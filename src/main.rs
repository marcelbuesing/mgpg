use async_std::task;
use colored::*;
use dialoguer::{theme::ColorfulTheme, Input, PasswordInput};
use dirs::config_dir;
use gpgme::{Context, Protocol};
use keyring;
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::path::PathBuf;
use structopt::StructOpt;
use surf;
use thiserror::Error as ThisError;

const KEYRING_SERVICE: &str = "mattercryptclient";
const SETTINGS_FILE_NAME: &str = "mcc";

#[derive(ThisError, Debug)]
enum Error {
    #[error("HTTP request error {}", .0)]
    Surf(#[from] surf::Exception),
    #[error("Deserialization error {}", .0)]
    Serde(#[from] serde_json::Error),
    #[error("Token was not returned as expected from server")]
    TokenMissing,
    #[error("Gpg error {}", .0)]
    Gpg(#[from] gpgme::Error),
    #[error("IO error {}", .0)]
    Io(#[from] io::Error),
    #[error("Key UTF8 decoding error {:?}", .0)]
    KeyUtf8(#[from] Option<std::str::Utf8Error>),
    #[error("Keyring error {}", .0)]
    Keyring(#[from] keyring::KeyringError),
}

#[derive(StructOpt, Debug)]
#[structopt(name = "mattercrypt")]
struct Opt {
    #[structopt(short, long)]
    to: Vec<String>,
    #[structopt(short, long)]
    sign: bool,
    #[structopt(short, long, parse(from_os_str))]
    file: Option<PathBuf>,
    #[structopt(long)]
    reinit: bool,
    #[structopt()]
    message: Option<String>,
}

#[derive(Debug)]
struct Token(String);

#[derive(Debug)]
struct ChannelId(String);

#[derive(Debug, Deserialize)]
struct User {
    /// User id
    id: String,
    email: String,
    first_name: String,
    last_name: String,
    nickname: String,
}

#[derive(Debug)]
struct Settings {
    api_url: String,
    username: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredSettings {
    api_url: String,
    username: String,
}

/// Retrieve API token using user credentials
async fn get_token(settings: &Settings) -> Result<(Token, User), Error> {
    let data = serde_json::json!({ "login_id": settings.username,"password": settings.password });

    let uri = format!("{}/users/login", settings.api_url);
    let mut response = surf::post(uri).body_json(&data)?.await?;
    let token = response.header("Token").ok_or(Error::TokenMissing)?;
    let token = format!("Bearer {}", token);

    let user_details = response.body_json::<User>().await?;

    Ok((Token(token), user_details))
}

/// Retrieve user by email address
async fn get_user(settings: &Settings, token: &Token, email: &str) -> Result<User, Error> {
    let uri = format!("{}/users/email/{}", settings.api_url, email);
    let user = surf::get(uri)
        .set_header("Authorization", token.0.clone())
        .recv_json()
        .await?;
    Ok(user)
}

/// Create a message channel between sender and recipient
async fn create_direct_message_channel(
    settings: &Settings,
    token: &Token,
    from: &str,
    to: &str,
) -> Result<ChannelId, Error> {
    let data = serde_json::json!(&[from, to]);
    let uri = format!("{}/channels/direct", settings.api_url);
    let response = surf::post(uri)
        .set_header("Authorization", token.0.clone())
        .body_json(&data)?
        .recv_string()
        .await?;
    let v: Value = serde_json::from_str(&response)?;
    let channel_id = ChannelId(v["id"].as_str().unwrap().to_string());
    Ok(channel_id)
}

/// Send message to channel (recipient)
async fn create_post(
    settings: &Settings,
    token: &Token,
    channel_id: &ChannelId,
    message: &str,
) -> Result<(), Error> {
    let data = serde_json::json!({
        "channel_id": channel_id.0,
        // "file_ids":[],
        "message": message,
    });

    let uri = format!("{}/posts", settings.api_url);
    surf::post(uri)
        .set_header("Authorization", token.0.clone())
        .body_json(&data)?
        .recv_string()
        .await?;
    Ok(())
}

/// Retrieve API token, encrypt message per recipient and send it to each recipient
async fn send_message(settings: &Settings, opt: &Opt, message: &str) -> Result<(), Error> {
    let (token, user_details) = get_token(settings).await?;

    let mut ctx = Context::from_protocol(Protocol::OpenPgp)?;
    ctx.set_armor(true);

    for recipient in opt.to.iter() {
        // Encrypt message per recipient
        let public_key = ctx.get_key(recipient)?;
        let mut ciphertext = Vec::new();
        if opt.sign {
            ctx.sign_and_encrypt(Some(&public_key), message, &mut ciphertext)?;
        } else {
            ctx.encrypt(Some(&public_key), message, &mut ciphertext)?;
        }

        let recipient_user = get_user(settings, &token, &recipient).await?;
        let channel_id =
            create_direct_message_channel(settings, &token, &user_details.id, &recipient_user.id)
                .await?;

        let cipherstring = std::str::from_utf8(&ciphertext).unwrap();
        let message = format!("```\necho \"\n{}\" | gpg --decrypt\n```", cipherstring);

        create_post(settings, &token, &channel_id, &message).await?;

        print!(
            "{} Successfully sent message\nFROM:\t{}\nTO:\t{}\nFINGERPRINT: {}\nMESSAGE:\n{}\n",
            "✓".green(),
            user_details.email.magenta(),
            recipient_user.email.cyan(),
            public_key.fingerprint()?.cyan(),
            message
        );
    }

    Ok(())
}

/// Store config setup including credentials
fn init_settings() -> Result<Settings, Error> {
    println!("Initialize settings:");

    let api_url: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("API Url (e.g. https://my-mattermost-server.com/api/v4)")
        .interact()?;

    let username: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Login username")
        .interact()?;

    let password: String = PasswordInput::with_theme(&ColorfulTheme::default())
        .with_prompt("Login Password (will be securely stored in Keyring)")
        .with_confirmation("Repeat password", "Error: the passwords don't match.")
        .interact()?;

    let keyring = keyring::Keyring::new(KEYRING_SERVICE, &username);
    keyring.set_password(&password)?;

    let stored_settings = StoredSettings {
        api_url: api_url.clone(),
        username: username.clone(),
    };
    let serialized_settings = serde_json::to_vec_pretty(&stored_settings)?;
    let mut settings_path = config_dir().unwrap_or_default();
    settings_path.push(SETTINGS_FILE_NAME);
    let mut file = File::create(settings_path)?;
    file.write_all(&serialized_settings)?;

    Ok(Settings {
        api_url,
        username,
        password,
    })
}

/// Read config setup including credentials
fn load_settings() -> Result<Settings, Error> {
    let mut settings_path = config_dir().unwrap_or_default();
    settings_path.push(SETTINGS_FILE_NAME);
    let mut file = File::open(settings_path)?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)?;
    let stored_settings: StoredSettings = serde_json::from_slice(&content)?;

    let keyring = keyring::Keyring::new(KEYRING_SERVICE, &stored_settings.username);
    let password = keyring.get_password()?;

    Ok(Settings {
        api_url: stored_settings.api_url,
        username: stored_settings.username,
        password,
    })
}

fn main() -> Result<(), Error> {
    let opt = Opt::from_args();

    if opt.reinit {
        init_settings()?;
    }

    // Load settings and init when it does not exist yet
    let settings = match load_settings() {
        Err(Error::Io(io_err)) => {
            if io_err.kind() != ErrorKind::NotFound {
                return Err(Error::Io(io_err));
            }
            init_settings()?
        }
        config => config?,
    };

    let message: Result<String, Error> = match opt.message {
        None => {
            // Read message from stdin if it's not passed as parameter
            let stdin = io::stdin();
            let mut handle = stdin.lock();

            let mut message = String::new();
            handle.read_to_string(&mut message)?;

            Ok(message)
        }
        Some(ref message) => Ok(message.to_string()),
    };
    task::block_on(send_message(&settings, &opt, &message?))?;

    Ok(())
}
