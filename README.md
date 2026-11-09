# fog — ftp over gist

Send a file to another person through a secret GitHub gist. fog encrypts the
file with a passcode before upload. The receiver needs the gist link and the
passcode. The receiver does not need a GitHub account.

> **Acceptable use.** GitHub
> [Acceptable Use Policies](https://docs.github.com/en/site-policy/acceptable-use-policies/github-acceptable-use-policies)
> do not permit gists as a file-hosting service. Use fog only for small,
> occasional transfers. fog shows this warning on every run and does nothing
> until you type `y`.

## Install

You need [mise](https://mise.jdx.dev). `mise.toml` pins the Rust toolchain.

```sh
mise install
make release            # binary: target/release/fog
make install            # or: install fog into ~/.cargo/bin
```

To send, you need a GitHub token. fog uses `GITHUB_TOKEN` if it is set. If
not, fog uses `gh auth token`, so `gh auth login` is sufficient. To receive,
you do not need a token.

## Usage

### Send

```sh
fog send ./report.pdf
```

fog prints the gist URL and a generated passcode. Give the URL and the
passcode to the receiver on different channels. To set your own passcode,
use `--passcode <text>`.

| Flag | Default | Effect |
|---|---|---|
| `--passcode <text>` | generated, 24 characters | The passcode for encryption. |
| `--chunk-size <bytes>` | 8388608 (8 MiB) | Bytes of encrypted text in one gist file. The range is 1024 to 10000000. |
| `--unlock-unlimited-power` | off | Raises the part limit from 6 to 298. \*limits apply |

### Receive

```sh
fog recv https://gist.github.com/<user>/<id>
```

fog asks for the passcode. Or, use `--passcode <text>`. fog writes the file
with its original name into the current directory. To set a different path,
use `-o <path>`. fog does not overwrite a file that exists.

### Delete

```sh
fog rm https://gist.github.com/<user>/<id>
```

Only the sender can delete the gist. Delete the gist after the receiver has
the file.

## Limits

| Limit | Value | Cause |
|---|---|---|
| Parts per transfer (default) | 6, about **35 MiB** of input | fog default |
| Parts per transfer with `--unlock-unlimited-power` | 298, about **1.7 GiB** of input | The GitHub API lists a maximum of 300 files in a gist. fog uses 2 of them. |
| Size of one part | 10,000,000 bytes maximum | GitHub serves a maximum of 10 MB from a gist raw URL. |
| Uploads | 80 per minute, 500 per hour | GitHub limit for content creation. fog uploads one part per request. When GitHub sends a rate-limit response, fog waits and tries again (5 attempts maximum). |
| Receiver API calls | 60 per hour per IP address | GitHub limit for requests without a token. Each receive uses 1 API call. Downloads of parts are not API calls. |

Encryption makes the data approximately 35% larger: base64 adds 33% and line
breaks add approximately 1.5%. Thus, 6 parts of 8 MiB hold approximately
35 MiB of input.

## How it works

1. `send` encrypts the file with [age](https://age-encryption.org) (scrypt
   passphrase mode). The output is in ASCII armor, which is base64 text.
2. fog cuts the armored text into parts at line ends. Each part becomes one
   file in a single secret gist: `part-0000`, `part-0001`, and so on.
3. The original file name is encrypted separately into `name.age`. Thus, the
   gist shows only the size of the file.
4. `fog.json` is written last. It records the number of parts. If
   `fog.json` is not in the gist, the upload is not complete.
5. `recv` makes one API call to get the list of files. Then it downloads the
   parts from their raw URLs and decrypts them as a stream. Before it
   downloads the parts, it decrypts `name.age`, which checks the passcode.
   Output goes to `<name>.fog-partial` and fog renames it only after the
   decryption is complete and correct.

A secret gist is not private. Any person who has the URL can download the
encrypted data. The passcode is the only protection, so use a strong passcode.

## Development

```sh
make test   # round-trip test, no network
make lint   # rustfmt check + clippy with -D warnings
```
