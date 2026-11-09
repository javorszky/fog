mod gist;

use age::armor::{ArmoredReader, ArmoredWriter, Format};
use age::secrecy::SecretString;
use anyhow::{Context, Result, bail};
use clap::Parser;
use std::fs::{self, File};
use std::io::{self, BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};

/// raw_url serves up to 10 MB per file; stay below it.
const DEFAULT_CHUNK: usize = 8 * 1024 * 1024;
/// API lists at most 300 files; leave room for fog.json and name.age.
const MAX_PARTS: usize = 298;

#[derive(Parser)]
#[command(about = "Send files through encrypted secret GitHub gists")]
enum Cli {
    /// Encrypt and upload a file; prints the gist URL and passcode.
    Send {
        path: PathBuf,
        #[arg(long)]
        passcode: Option<String>,
        /// Bytes of armored text per gist file.
        #[arg(long, default_value_t = DEFAULT_CHUNK)]
        chunk_size: usize,
    },
    /// Download and decrypt a file from a gist URL or id.
    Recv {
        gist: String,
        #[arg(long)]
        passcode: Option<String>,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Delete a gist created by `send`.
    Rm { gist: String },
}

fn main() -> Result<()> {
    match Cli::parse() {
        Cli::Send {
            path,
            passcode,
            chunk_size,
        } => send(&path, passcode, chunk_size),
        Cli::Recv {
            gist,
            passcode,
            out,
        } => recv(gist::parse_id(&gist), passcode, out),
        Cli::Rm { gist } => gist::Gist::new(true)?.delete(gist::parse_id(&gist)),
    }
}

fn send(path: &Path, passcode: Option<String>, chunk: usize) -> Result<()> {
    if chunk < 1024 {
        bail!("--chunk-size must be at least 1024");
    }
    let size = fs::metadata(path)?.len() as usize;
    // armor: base64 (4/3) plus one newline per 64 chars, plus header/footer slack
    let parts = (size / 3 * 4 * 65 / 64 + 4096).div_ceil(chunk);
    if parts > MAX_PARTS {
        bail!(
            "file needs ~{parts} parts, max is {MAX_PARTS}; raise --chunk-size or split the file"
        );
    }
    let name = path
        .file_name()
        .context("path has no file name")?
        .to_string_lossy();
    let pass = passcode.unwrap_or_else(gen_passcode);
    let secret = SecretString::from(pass.clone());

    let mut name_ct = String::new();
    encrypt(name.as_bytes(), &secret, usize::MAX, |c| {
        name_ct.push_str(c);
        Ok(())
    })?;

    let g = gist::Gist::new(true)?;
    let mut created: Option<(String, String)> = None;
    let mut n = 0;
    encrypt(File::open(path)?, &secret, chunk, |c| {
        let part = format!("part-{n:04}");
        eprintln!("uploading {part} ({} bytes)", c.len());
        match &created {
            None => {
                created = Some(
                    g.create(&[("name.age", &name_ct), (&part, c)])
                        .map_err(io::Error::other)?,
                )
            }
            Some((id, _)) => g.add(id, &[(&part, c)]).map_err(io::Error::other)?,
        }
        n += 1;
        Ok(())
    })?;
    let (id, url) = created.context("nothing was uploaded")?;
    g.add(&id, &[("fog.json", &format!(r#"{{"v":1,"chunks":{n}}}"#))])?;
    println!("{url}\npasscode: {pass}");
    Ok(())
}

fn recv(id: &str, passcode: Option<String>, out: Option<PathBuf>) -> Result<()> {
    let g = gist::Gist::new(false)?;
    let files = g.files(id)?;
    let manifest = files
        .get("fog.json")
        .context("not a fog gist, or upload not finished")?;
    let manifest: serde_json::Value = serde_json::from_slice(&g.raw(manifest)?)?;
    let n = manifest["chunks"].as_u64().context("bad fog.json")? as usize;
    let urls = (0..n)
        .map(|i| {
            files
                .get(&format!("part-{i:04}"))
                .with_context(|| format!("part {i} missing"))
        })
        .collect::<Result<Vec<_>>>()?;

    let pass = match passcode {
        Some(p) => p,
        None => rpassword::prompt_password("passcode: ")?,
    };
    let secret = SecretString::from(pass);

    // Checks the passcode before the big download.
    let mut name = Vec::new();
    let name_url = files.get("name.age").context("name.age missing")?;
    decrypt(Cursor::new(g.raw(name_url)?), &secret, &mut name).context("wrong passcode?")?;
    // The name is untrusted: keep only the final component, no paths.
    let name = String::from_utf8(name)?;
    let out = match out {
        Some(o) => o,
        None => PathBuf::from(
            Path::new(&name)
                .file_name()
                .context("bad file name in gist")?,
        ),
    };
    if out.exists() {
        bail!("{} exists, refusing to overwrite", out.display());
    }

    let tmp = PathBuf::from(format!("{}.fog-partial", out.display()));
    let mut urls = urls.into_iter();
    let parts = Parts::new(|| {
        urls.next().map(|u| {
            eprintln!("downloading {u}");
            g.raw(u).map_err(io::Error::other)
        })
    });
    let res = decrypt(parts, &secret, File::create(&tmp)?);
    if let Err(e) = res {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, &out)?;
    println!("{}", out.display());
    Ok(())
}

fn gen_passcode() -> String {
    const A: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789"; // no look-alikes
    (0..24)
        .map(|_| A[rand::random_range(0..A.len())] as char)
        .collect()
}

/// Encrypts `input` with age (scrypt) as ASCII armor, calling `sink` with
/// chunks of at most `chunk` bytes, cut at line ends.
fn encrypt(
    mut input: impl Read,
    pass: &SecretString,
    chunk: usize,
    sink: impl FnMut(&str) -> io::Result<()>,
) -> Result<()> {
    let chunker = Chunker {
        buf: Vec::new(),
        chunk,
        sink,
    };
    let armor = ArmoredWriter::wrap_output(chunker, Format::AsciiArmor)?;
    let mut w = age::Encryptor::with_user_passphrase(pass.clone()).wrap_output(armor)?;
    io::copy(&mut input, &mut w)?;
    w.finish()?.finish()?.flush_rest()?;
    Ok(())
}

fn decrypt(input: impl Read, pass: &SecretString, mut out: impl Write) -> Result<()> {
    let d = age::Decryptor::new_buffered(ArmoredReader::new(BufReader::new(input)))?;
    let id = age::scrypt::Identity::new(pass.clone());
    let mut r = d.decrypt(std::iter::once(&id as &dyn age::Identity))?;
    io::copy(&mut r, &mut out)?;
    out.flush()?;
    Ok(())
}

struct Chunker<F> {
    buf: Vec<u8>,
    chunk: usize,
    sink: F,
}

impl<F: FnMut(&str) -> io::Result<()>> Chunker<F> {
    fn emit(&mut self, end: usize) -> io::Result<()> {
        // Armor is ASCII, so this never fails.
        (self.sink)(std::str::from_utf8(&self.buf[..end]).map_err(io::Error::other)?)?;
        self.buf.drain(..end);
        Ok(())
    }

    fn flush_rest(mut self) -> io::Result<()> {
        if !self.buf.is_empty() {
            self.emit(self.buf.len())?;
        }
        Ok(())
    }
}

impl<F: FnMut(&str) -> io::Result<()>> Write for Chunker<F> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        while self.buf.len() > self.chunk {
            // Cut at a line end: a stripped trailing newline can then be restored.
            let end = self.buf[..self.chunk]
                .iter()
                .rposition(|&b| b == b'\n')
                .ok_or_else(|| io::Error::other("no line end in chunk"))?
                + 1;
            self.emit(end)?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Concatenates downloaded parts; adds a newline to any part that lost it.
struct Parts<F> {
    next: F,
    cur: Cursor<Vec<u8>>,
}

impl<F> Parts<F> {
    fn new(next: F) -> Self {
        Self {
            next,
            cur: Cursor::new(Vec::new()),
        }
    }
}

impl<F: FnMut() -> Option<io::Result<Vec<u8>>>> Read for Parts<F> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        loop {
            let n = self.cur.read(out)?;
            if n > 0 || out.is_empty() {
                return Ok(n);
            }
            match (self.next)() {
                None => return Ok(0),
                Some(part) => {
                    let mut part = part?;
                    if part.last() != Some(&b'\n') {
                        part.push(b'\n');
                    }
                    self.cur = Cursor::new(part);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_many_chunks() {
        let data: Vec<u8> = (0..20_000).map(|_| rand::random()).collect();
        let pass = SecretString::from("pw".to_string());
        let mut parts: Vec<String> = Vec::new();
        encrypt(&data[..], &pass, 1024, |c| {
            parts.push(c.to_string());
            Ok(())
        })
        .unwrap();
        assert!(parts.len() > 20 && parts.iter().all(|p| p.len() <= 1024));

        // Simulate GitHub stripping trailing newlines.
        let mut it = parts.iter().map(|p| Ok(p.trim_end().as_bytes().to_vec()));
        let mut out = Vec::new();
        decrypt(Parts::new(|| it.next()), &pass, &mut out).unwrap();
        assert_eq!(out, data);

        let mut it = parts.iter().map(|p| Ok(p.as_bytes().to_vec()));
        let bad = SecretString::from("nope".to_string());
        assert!(decrypt(Parts::new(|| it.next()), &bad, io::sink()).is_err());
    }

    #[test]
    fn parse_id() {
        assert_eq!(
            gist::parse_id("https://gist.github.com/u/abc123/"),
            "abc123"
        );
        assert_eq!(gist::parse_id("abc123"), "abc123");
    }
}
