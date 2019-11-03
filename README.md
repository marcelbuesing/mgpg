<div align="center">

  <h1>📦✨  mgpg</h1>

  <p>
    <strong>Mattermost Crypto Client</strong>
  </p>

  <p>
    <a href="https://github.com/marcelbuesing/mgpg/actions?query=workflow%3ACI"><img alt="Build Status" src="https://github.com/marcelbuesing/mgpg/workflows/CI/badge.svg"/></a>
    <a href="https://crates.io/crates/mgpg"><img alt="crates.io" src="https://meritbadge.herokuapp.com/mgpg"/></a>
  </p>

  <h3>
    <a href="https://docs.rs/mgpg">Docs</a>
  </h3>

  <sub>Built with 🦀</sub>
</div>

# About

A mattermost client for conveniently encrypting messages using GnuPG via [GPGME](https://gnupg.org/software/gpgme/index.html).

# Setup

Install from source:
```
cargo install mgpg
```

When running mgpg for the first time you'll be guided through a setup process.
Your Mattermost password will be securely stored in your "keyring".
Other configuration values are stored in "~./config/mgpg" in plain format.

To rerun the setup process, replacing all previous values, run `mgpg --reinit`.

# Usage
Make sure GPG is aware of the recipient's public key by importing the key.
You can verify that the public key has been imported via `gpg --fingerprint recipient@mail.com` or alternatively, check the output of `gpg --list-keys`.

Encrypt message using the public key of the recipient and send it as a direct message to the recipient:
```
echo "In God we trust. The rest we monitor." | mgpg --to edward.lyle@mail.com
```

In addition to encrypting messages you may also sign them, before sending them:
```
echo "It's a brave new world out there." | mgpg --sign --to robert.dean@mail.com
```

Pass message as parameter:
```
mgpg --sign --to edward.lyle@mail.com -- "In God we trust. The rest we monitor."
```

# Help

```
mgpg --help
mgpg 0.1.0

USAGE:
    mgpg [FLAGS] [OPTIONS] [--] [message]

FLAGS:
    -h, --help       Prints help information
        --reinit
    -s, --sign
    -V, --version    Prints version information

OPTIONS:
    -f, --file <file>
    -t, --to <to>...

ARGS:
    <message>
```