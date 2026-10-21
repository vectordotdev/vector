//! Native package validation, `OpenPGP` signing and incremental repository metadata.
use super::{
    inputs::{DEB_ARCHES, RPM_ARCHES, verify_input},
    model::{
        Config, Entry, Snapshot, copy, digest, entry_value, files, mutable, safe_relative, string,
        write,
    },
};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Timelike, Utc};
use flate2::{Compression, GzBuilder};
use roxmltree::{Document, Node};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::fmt::Write as _;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    // Never leak signing material or cloud credentials to packaging subprocesses.
    for name in [
        "PACKAGE_SIGNING_SUBKEY",
        "PACKAGE_SIGNING_PASSPHRASE",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "GH_TOKEN",
    ] {
        command.env_remove(name);
    }
    command
}
pub(super) fn run(command: &mut Command, input: Option<&[u8]>) -> Result<Vec<u8>> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("Cannot start {}", command.get_program().to_string_lossy()))?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .context("Missing command stdin")?
            .write_all(input)?;
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "{} failed: {}",
        command.get_program().to_string_lossy(),
        String::from_utf8_lossy(&output.stderr)
            .chars()
            .rev()
            .take(4000)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    );
    Ok(output.stdout)
}
fn text(command: &mut Command) -> Result<String> {
    Ok(String::from_utf8(run(command, None)?)?)
}
pub(super) fn fields(paragraph: &str) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for line in paragraph.lines() {
        if line.starts_with([' ', '\t']) {
            continue;
        }
        let (key, value) = line.split_once(':').context("Malformed package metadata")?;
        ensure!(
            result
                .insert(key.to_owned(), value.trim().to_owned())
                .is_none(),
            "Duplicate metadata field"
        );
    }
    Ok(result)
}
pub(super) fn merge_packages(previous: &[u8], incoming: &[u8]) -> Result<Vec<u8>> {
    let mut packages: BTreeMap<(String, String, String), &str> = BTreeMap::new();
    for data in [previous, incoming] {
        for paragraph in std::str::from_utf8(data)?
            .trim()
            .split("\n\n")
            .filter(|p| !p.is_empty())
        {
            let item = fields(paragraph)?;
            ensure!(
                item.get("Package").map(String::as_str) == Some("vector"),
                "Foreign Debian package"
            );
            let key = (
                item.get("Package").context("Missing name")?.clone(),
                item.get("Version").context("Missing version")?.clone(),
                item.get("Architecture")
                    .context("Missing architecture")?
                    .clone(),
            );
            if let Some(old) = packages.get(&key) {
                ensure!(fields(old)? == item, "Conflicting Debian identity");
            }
            packages.insert(key, paragraph);
        }
    }
    let mut result = Vec::new();
    for paragraph in packages.values() {
        result.extend_from_slice(paragraph.as_bytes());
        result.extend_from_slice(b"\n\n");
    }
    Ok(result)
}
pub(super) fn lint_package(path: &Path, identity: &Value) -> Result<()> {
    if identity["format"] == "deb" {
        for (field, expected) in [
            ("Package", "vector"),
            ("Version", string(identity, "version")?),
            ("Architecture", string(identity, "arch")?),
        ] {
            ensure!(
                text(command("dpkg-deb").arg("-f").arg(path).arg(field))?.trim() == expected,
                "Debian native identity mismatch"
            );
        }
        let mut child = command("dpkg-deb")
            .arg("--fsys-tarfile")
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let result = (|| -> Result<()> {
            let mut archive =
                tar::Archive::new(child.stdout.take().context("Missing dpkg stdout")?);
            let mut size: u64 = 0;
            for member in archive.entries()? {
                let member = member?;
                size = size
                    .checked_add(member.size())
                    .context("Package size overflow")?;
                ensure!(size < 8 * 1024 * 1024 * 1024, "Unreasonably large payload");
                let kind = member.header().entry_type();
                ensure!(
                    kind.is_symlink()
                        || kind.is_hard_link()
                        || member.header().mode()? & 0o002 == 0,
                    "World-writable Debian entry"
                );
            }
            Ok(())
        })();
        if result.is_err() {
            drop(child.kill());
        }
        let status = child.wait()?;
        result?;
        ensure!(status.success(), "Cannot inspect Debian payload");
    } else {
        ensure!(identity["format"] == "rpm", "Unsupported package format");
        let actual = text(
            command("rpm")
                .args(["-qp", "--qf", "%{NAME} %{VERSION}-%{RELEASE} %{ARCH}"])
                .arg(path),
        )?;
        ensure!(
            actual
                == format!(
                    "vector {} {}",
                    string(identity, "version")?,
                    string(identity, "arch")?
                ),
            "RPM native identity mismatch"
        );
        for line in text(
            command("rpm")
                .args(["-qp", "--qf", "[%{FILEMODES:perms} %{FILENAMES}\\n]"])
                .arg(path),
        )?
        .lines()
        {
            ensure!(
                line.len() >= 10 && (line.starts_with('l') || line.as_bytes()[8] != b'w'),
                "Invalid or world-writable RPM entry"
            );
        }
    }
    Ok(())
}
fn fingerprints(data: &[u8], primary: &str, subkey: &str) -> Result<Vec<(Vec<String>, String)>> {
    let mut pending = None;
    let mut result = Vec::new();
    for line in std::str::from_utf8(data)?.lines() {
        let parts: Vec<_> = line.split(':').map(str::to_owned).collect();
        if parts[0] == primary || parts[0] == subkey {
            pending = Some(parts);
        } else if parts[0] == "fpr"
            && let Some(key) = pending.take()
        {
            result.push((key, parts.get(9).context("Missing fingerprint")?.clone()));
        }
    }
    Ok(result)
}
pub(super) struct PublicKey<'a> {
    pub config: &'a Config,
    pub home: PathBuf,
}
impl<'a> PublicKey<'a> {
    pub fn new(config: &'a Config, name: &str) -> Result<Self> {
        use std::os::unix::fs::DirBuilderExt as _;
        let home = Path::new("/run/keys").join(name);
        fs::DirBuilder::new().mode(0o700).create(&home)?;
        let public = Self { config, home };
        let keys = fingerprints(
            &public.gpg(
                [
                    OsStr::new("--with-colons"),
                    OsStr::new("--show-keys"),
                    config.key_path.as_os_str(),
                ],
                None,
            )?,
            "pub",
            "sub",
        )?;
        ensure!(
            keys.iter()
                .filter(|(p, _)| p[0] == "pub")
                .map(|(_, f)| f.as_str())
                .collect::<Vec<_>>()
                == [config.primary_fingerprint.as_str()],
            "Wrong public primary key"
        );
        ensure!(
            keys.iter()
                .any(|(p, f)| p[0] == "sub" && *f == config.signing_fingerprint),
            "Wrong signing subkey"
        );
        public.gpg([OsStr::new("--import"), config.key_path.as_os_str()], None)?;
        Ok(public)
    }
    fn gpg_command(&self) -> Command {
        let mut cmd = command("gpg");
        cmd.args(["--batch", "--no-tty"])
            .env("GNUPGHOME", &self.home);
        cmd
    }
    pub fn gpg(
        &self,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        input: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        run(self.gpg_command().args(args), input)
    }
    pub fn verify(&self, signature: &Path, data: Option<&Path>) -> Result<()> {
        let mut cmd = self.gpg_command();
        cmd.args(["--status-fd", "1", "--verify"]).arg(signature);
        if let Some(data) = data {
            cmd.arg(data);
        }
        let status = text(&mut cmd)?;
        ensure!(
            status.lines().any(|l| l.starts_with(&format!(
                "[GNUPG:] VALIDSIG {} ",
                self.config.signing_fingerprint
            ))),
            "Signature did not use pinned subkey"
        );
        Ok(())
    }
    pub fn verify_apt_release(&self, suite: &Path) -> Result<Vec<u8>> {
        self.verify(&suite.join("InRelease"), None)?;
        self.verify(&suite.join("Release.gpg"), Some(&suite.join("Release")))?;
        let release = fs::read(suite.join("Release"))?;
        ensure!(
            self.gpg(
                [OsStr::new("--decrypt"), suite.join("InRelease").as_os_str()],
                None
            )? == release,
            "Inline and detached roots disagree"
        );
        Ok(release)
    }
    pub fn rpmdb(&self) -> Result<PathBuf> {
        let path = self.home.join("rpmdb");
        run(
            command("rpm")
                .arg("--dbpath")
                .arg(&path)
                .arg("--import")
                .arg(&self.config.key_path),
            None,
        )?;
        Ok(path)
    }
}
impl Drop for PublicKey<'_> {
    fn drop(&mut self) {
        drop(
            command("gpgconf")
                .args(["--kill", "all"])
                .env("GNUPGHOME", &self.home)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status(),
        );
        drop(fs::remove_dir_all(&self.home));
    }
}
pub(super) struct Signer<'a> {
    pub public: PublicKey<'a>,
    password: PathBuf,
    pub rpmdb: PathBuf,
}
impl<'a> Signer<'a> {
    pub fn new(config: &'a Config, name: &str) -> Result<Self> {
        use std::os::unix::fs::OpenOptionsExt as _;
        let private =
            std::env::var("PACKAGE_SIGNING_SUBKEY").context("Missing encrypted signing subkey")?;
        let passphrase =
            std::env::var("PACKAGE_SIGNING_PASSPHRASE").context("Missing signing passphrase")?;
        ensure!(
            !private.is_empty() && !passphrase.is_empty() && !passphrase.contains(['\r', '\n']),
            "Expected encrypted subkey and single-line passphrase"
        );
        let public = PublicKey::new(config, name)?;
        let password = public.home.join("passphrase");
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&password)?;
        writeln!(output, "{passphrase}")?;
        drop(output);
        public.gpg(["--import"], Some(private.as_bytes()))?;
        let keys = fingerprints(
            &public.gpg(["--with-colons", "--list-secret-keys"], None)?,
            "sec",
            "ssb",
        )?;
        ensure!(
            keys.len() == 2,
            "Expected exactly one primary stub and one signing subkey"
        );
        let (primary, pf) = &keys[0];
        let (subkey, sf) = &keys[1];
        ensure!(
            *pf == config.primary_fingerprint
                && primary[0] == "sec"
                && primary.get(14).map(String::as_str) == Some("#"),
            "Usable primary secret key is forbidden in CI"
        );
        ensure!(
            *sf == config.signing_fingerprint
                && subkey[0] == "ssb"
                && subkey.get(14).map(String::as_str) == Some("+")
                && subkey.get(11).is_some_and(|s| s.contains('s')),
            "Wrong or unavailable signing subkey"
        );
        for (parts, _) in &keys {
            ensure!(
                !matches!(parts.get(1).map(String::as_str), Some("r" | "e" | "d"))
                    && parts.get(6).context("Missing key expiry")?.parse::<i64>()?
                        > Utc::now().timestamp(),
                "Expired or revoked signing identity"
            );
        }
        let challenge = public.home.join("challenge");
        fs::write(&challenge, b"Vector repository signing capability check\n")?;
        run(
            command("gpgconf")
                .args(["--kill", "gpg-agent"])
                .env("GNUPGHOME", &public.home),
            None,
        )?;
        let wrong = run(
            public
                .gpg_command()
                .args([
                    "--pinentry-mode",
                    "loopback",
                    "--passphrase-fd",
                    "0",
                    "--local-user",
                ])
                .arg(format!("{}!", config.signing_fingerprint))
                .arg("--output")
                .arg(public.home.join("wrong-passphrase.asc"))
                .arg("--detach-sign")
                .arg(&challenge),
            Some(
                format!(
                    "{}{}\n",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                )
                .as_bytes(),
            ),
        );
        ensure!(
            wrong.is_err(),
            "Signing subkey must be passphrase-protected"
        );
        let rpmdb = public.rpmdb()?;
        let signer = Self {
            public,
            password,
            rpmdb,
        };
        signer.sign(&challenge, false, &challenge.with_extension("asc"))?;
        signer
            .public
            .verify(&challenge.with_extension("asc"), Some(&challenge))?;
        Ok(signer)
    }
    pub fn sign(&self, source: &Path, clear: bool, destination: &Path) -> Result<()> {
        run(
            self.public
                .gpg_command()
                .args(["--yes", "--pinentry-mode", "loopback", "--passphrase-file"])
                .arg(&self.password)
                .arg("--local-user")
                .arg(format!("{}!", self.public.config.signing_fingerprint))
                .args(["--digest-algo", "SHA256", "--armor", "--output"])
                .arg(destination)
                .arg(if clear {
                    "--clearsign"
                } else {
                    "--detach-sign"
                })
                .arg(source),
            None,
        )?;
        Ok(())
    }
    pub fn rpm(&self, package: &Path) -> Result<()> {
        run(
            command("rpmsign")
                .arg("--define")
                .arg(format!(
                    "_gpg_name {}!",
                    self.public.config.signing_fingerprint
                ))
                .arg("--define")
                .arg(format!("_gpg_path {}", self.public.home.display()))
                .args([
                    "--define",
                    "__gpg /usr/bin/gpg",
                    "--define",
                    "_gpg_digest_algo sha256",
                    "--define",
                ])
                .arg(format!(
                    "_gpg_sign_cmd_extra_args --pinentry-mode loopback --passphrase-file {}",
                    self.password.display()
                ))
                .arg("--addsign")
                .arg(package)
                .env("GNUPGHOME", &self.public.home),
            None,
        )?;
        verify_rpm(&self.rpmdb, package)
    }
}
pub(super) fn verify_rpm(database: &Path, package: &Path) -> Result<()> {
    ensure!(
        text(
            command("rpm")
                .arg("--dbpath")
                .arg(database)
                .arg("--checksig")
                .arg(package)
        )?
        .contains("digests signatures OK"),
        "RPM signature verification failed"
    );
    Ok(())
}
fn child<'a>(node: Node<'a, 'a>, tag: &str) -> Result<Node<'a, 'a>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == tag)
        .with_context(|| format!("Missing XML {tag}"))
}
fn location<'a>(node: Node<'a, 'a>) -> Result<&'a str> {
    let location = child(node, "location")?;
    ensure!(
        location.attributes().len() == 1,
        "RPM locations must be relative"
    );
    let name = location.attribute("href").context("Missing href")?;
    safe_relative(&format!("yum/metadata/{name}"))?;
    Ok(name)
}
pub(super) fn repomd_locations(root: &Path) -> Result<Vec<String>> {
    let data = fs::read_to_string(root.join("repodata/repomd.xml"))?;
    let document = Document::parse(&data)?;
    let mut result = Vec::new();
    for entry in document
        .root_element()
        .children()
        .filter(|n| n.has_tag_name(("http://linux.duke.edu/metadata/repo", "data")))
    {
        let name = location(entry)?;
        ensure!(
            name.starts_with("repodata/") && name.split('/').count() == 2,
            "Invalid RPM metadata location"
        );
        result.push(name.to_owned());
    }
    ensure!(!result.is_empty(), "Empty RPM root metadata");
    Ok(result)
}
pub(super) fn check_apt_index(data: &[u8], snapshot: &Snapshot) -> Result<()> {
    for paragraph in std::str::from_utf8(data)?
        .trim()
        .split("\n\n")
        .filter(|p| !p.is_empty())
    {
        let item = fields(paragraph)?;
        ensure!(
            item.get("Package").map(String::as_str) == Some("vector"),
            "Foreign package in apt history"
        );
        let name = format!(
            "apt/{}",
            item.get("Filename").context("Missing package filename")?
        );
        safe_relative(&name)?;
        let entry = Entry {
            sha256: item
                .get("SHA256")
                .context("Missing package digest")?
                .clone(),
            size: item.get("Size").context("Missing package size")?.parse()?,
        };
        ensure!(
            snapshot.entries.get(&name) == Some(&entry),
            "Debian history differs from manifest"
        );
    }
    Ok(())
}
#[allow(clippy::too_many_lines)] // Keep the signed-root/index/payload checks together.
pub(super) fn verify_metadata(
    snapshot: &Snapshot,
    root: &Path,
    public: &PublicKey<'_>,
) -> Result<()> {
    for name in snapshot.entries.keys() {
        if name.ends_with("/InRelease") {
            let suite = root
                .join(name)
                .parent()
                .context("Missing suite")?
                .to_owned();
            let release = public.verify_apt_release(&suite)?;
            let mut sha256 = false;
            let mut checked = BTreeSet::new();
            for line in std::str::from_utf8(&release)?.lines() {
                if !line.starts_with(' ') {
                    sha256 = line == "SHA256:";
                } else if sha256 {
                    let pieces: Vec<_> = line.split_whitespace().collect();
                    ensure!(pieces.len() == 3, "Malformed Release checksum");
                    let relative = pieces[2];
                    safe_relative(&format!("apt/index/{relative}"))?;
                    let entry = Entry {
                        sha256: pieces[0].to_owned(),
                        size: pieces[1].parse()?,
                    };
                    let path = suite.join(relative);
                    let filename = path
                        .strip_prefix(root)?
                        .to_str()
                        .context("Invalid index path")?;
                    ensure!(
                        snapshot.entries.get(filename) == Some(&entry),
                        "Apt index differs from signed root"
                    );
                    if relative.ends_with("/Packages") {
                        let data = fs::read(path)?;
                        ensure!(Entry::bytes(&data) == entry, "Apt checksum mismatch");
                        check_apt_index(&data, snapshot)?;
                        checked.insert(relative);
                    }
                }
            }
            ensure!(!checked.is_empty(), "Apt root has no SHA-256 indexes");
        } else if name.ends_with("/repomd.xml") {
            let path = root.join(name);
            let package_root = path
                .parent()
                .and_then(Path::parent)
                .context("Invalid RPM root")?;
            public.verify(
                &PathBuf::from(format!("{}.asc", path.display())),
                Some(&path),
            )?;
            let data = fs::read_to_string(&path)?;
            let document = Document::parse(&data)?;
            for item in document
                .root_element()
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "data")
            {
                let path = package_root.join(location(item)?);
                let checksum = child(item, "checksum")?;
                let data = fs::read(&path)?;
                let algorithm = checksum.attribute("type").context("Missing hash type")?;
                ensure!(
                    matches!(algorithm, "sha256" | "sha512"),
                    "RPM metadata needs SHA-2"
                );
                ensure!(
                    snapshot
                        .entries
                        .get(path.strip_prefix(root)?.to_str().context("Invalid path")?)
                        == Some(&Entry::bytes(&data))
                        && hash(&data, algorithm) == checksum.text().context("Missing hash")?,
                    "RPM metadata differs from signed root"
                );
                if item.attribute("type") == Some("primary") {
                    ensure!(
                        path.extension() == Some(OsStr::new("gz")),
                        "Expected gzip primary metadata"
                    );
                    let mut expanded = String::new();
                    super::model::gzip(data.as_slice(), 512 * 1024 * 1024 + 1)
                        .read_to_string(&mut expanded)?;
                    ensure!(
                        expanded.len() <= 512 * 1024 * 1024,
                        "RPM primary metadata exceeds extraction budget"
                    );
                    let primary = Document::parse(&expanded)?;
                    for package in primary
                        .root_element()
                        .children()
                        .filter(|n| n.is_element() && n.tag_name().name() == "package")
                    {
                        ensure!(
                            child(package, "name")?.text() == Some("vector"),
                            "Foreign RPM package"
                        );
                        let path = package_root.join(location(package)?);
                        let checksum = child(package, "checksum")?;
                        ensure!(
                            checksum.attribute("type") == Some("sha256"),
                            "RPM package needs SHA-256"
                        );
                        let entry = Entry {
                            sha256: checksum.text().context("Missing checksum")?.to_owned(),
                            size: child(package, "size")?
                                .attribute("package")
                                .context("Missing package size")?
                                .parse()?,
                        };
                        ensure!(
                            snapshot
                                .entries
                                .get(path.strip_prefix(root)?.to_str().context("Invalid path")?)
                                == Some(&entry),
                            "RPM package differs from signed index"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
fn hash(data: &[u8], algorithm: &str) -> String {
    if algorithm == "sha256" {
        digest(data)
    } else {
        hex::encode(Sha512::digest(data))
    }
}
pub(super) fn release_date(data: &[u8]) -> Result<DateTime<Utc>> {
    let metadata = fields(std::str::from_utf8(data)?)?;
    ensure!(
        !metadata.contains_key("Valid-Until"),
        "Repository expiry requires a separate policy"
    );
    Ok(
        DateTime::parse_from_rfc2822(metadata.get("Date").context("Missing repository date")?)?
            .with_timezone(&Utc),
    )
}
pub(super) fn next_release_date(previous: &[Vec<u8>], now: DateTime<Utc>) -> Result<String> {
    let now = now.with_nanosecond(0).context("Invalid clock")?;
    let mut date = now;
    for data in previous {
        date = date.max(release_date(data)? + chrono::Duration::seconds(1));
    }
    ensure!(
        date <= now + chrono::Duration::seconds(5),
        "Repository date too far ahead; check clock and retry later"
    );
    Ok(date.to_rfc2822())
}
pub(super) fn dated_release(content: &[u8], date: &str) -> Result<Vec<u8>> {
    release_date(content)?;
    Ok(std::str::from_utf8(content)?
        .split_inclusive('\n')
        .map(|line| {
            if line.starts_with("Date:") {
                format!("Date: {date}\n")
            } else {
                line.to_owned()
            }
        })
        .collect::<String>()
        .into_bytes())
}
fn apt_indexes(
    root: &Path,
    channel: &str,
    incoming: &BTreeMap<String, Vec<u8>>,
    previous: &Path,
    signer: &Signer<'_>,
) -> Result<()> {
    let suite = root.join("apt/dists").join(channel);
    let architectures = ["amd64", "arm64", "armhf", "x86_64", "all", "i386"];
    debug_assert_eq!(&architectures[..3], &DEB_ARCHES);
    for arch in architectures {
        let relative = format!("apt/dists/{channel}/vector-0/binary-{arch}/Packages");
        let old = previous.join(&relative);
        let data = merge_packages(
            &if old.exists() {
                fs::read(old)?
            } else {
                Vec::new()
            },
            incoming
                .get(if arch == "x86_64" { "amd64" } else { arch })
                .map_or(&[], Vec::as_slice),
        )?;
        let index = root.join(relative);
        write(&index, &data)?;
        let mut compressed = GzBuilder::new()
            .mtime(0)
            .write(Vec::new(), Compression::best());
        compressed.write_all(&data)?;
        write(&index.with_extension("gz"), &compressed.finish()?)?;
        for path in [&index, &index.with_extension("gz")] {
            let content = fs::read(path)?;
            for algorithm in ["sha256", "sha512"] {
                write(
                    &path
                        .parent()
                        .context("Missing index parent")?
                        .join("by-hash")
                        .join(algorithm.to_uppercase())
                        .join(hash(&content, algorithm)),
                    &content,
                )?;
            }
        }
    }
    let prior = previous.join(format!("apt/dists/{channel}/Release"));
    let timestamp = next_release_date(
        &if prior.exists() {
            vec![fs::read(prior)?]
        } else {
            Vec::new()
        },
        Utc::now(),
    )?;
    // Keep the existing repository identity to avoid an additional apt approval.
    let mut release = format!(
        "Suite:\nCodename: {channel}\nDate: {timestamp}\nArchitectures: {}\nComponents: vector-0\nAcquire-By-Hash: yes\n",
        architectures.join(" ")
    );
    for algorithm in ["sha256", "sha512"] {
        writeln!(release, "{}:", algorithm.to_uppercase())?;
        for arch in architectures {
            for filename in ["Packages", "Packages.gz"] {
                let relative = format!("vector-0/binary-{arch}/{filename}");
                let data = fs::read(suite.join(&relative))?;
                writeln!(
                    release,
                    " {} {} {relative}",
                    hash(&data, algorithm),
                    data.len()
                )?;
            }
        }
    }
    write(&suite.join("Release"), release.as_bytes())?;
    signer.sign(&suite.join("Release"), true, &suite.join("InRelease"))?;
    signer.sign(&suite.join("Release"), false, &suite.join("Release.gpg"))?;
    Ok(())
}
#[allow(clippy::too_many_lines)] // Ordered admission, signing and final verification phase.
pub(super) fn prepare(config: &Config, work: &Path) -> Result<String> {
    let (manifest, input_id) = verify_input(work)?;
    ensure!(
        !work.join("result").exists(),
        "Prepared output exists; reuse it or choose another workspace"
    );
    let base_data = fs::read(work.join("base.json"))?;
    let previous = if base_data == b"null\n" {
        None
    } else {
        Some(Snapshot::decode(&base_data, None)?)
    };
    let base_files = work.join("base");
    for (name, path) in files(&base_files)? {
        ensure!(
            previous.as_ref().and_then(|s| s.entries.get(&name)) == Some(&Entry::file(&path)?),
            "Base metadata changed"
        );
    }
    let assets = manifest["assets"].as_object().context("Missing assets")?;
    for (name, identity) in assets {
        if identity.get("format").is_some() {
            lint_package(&work.join("input").join(name), identity)?;
        }
    }
    {
        let public = PublicKey::new(config, "verify")?;
        if let Some(previous) = &previous {
            verify_metadata(previous, &base_files, &public)?;
        }
    }
    let signer = Signer::new(config, "sign")?;
    let result = work.join("result");
    let root = result.join("files");
    fs::create_dir_all(&root)?;
    let mut entries = previous
        .as_ref()
        .map_or_else(BTreeMap::new, |s| s.entries.clone());
    for (name, source) in files(&base_files)? {
        copy(&source, &root.join(name))?;
    }
    let channel = string(&manifest, "channel")?;
    let mut incoming = BTreeMap::new();
    for (name, identity) in assets {
        if identity.get("format").is_none() {
            continue;
        }
        let is_deb = identity["format"] == "deb";
        let arch = string(identity, "arch")?;
        let relative = if is_deb {
            format!("apt/pool/v/ve/{name}")
        } else {
            format!("yum/{channel}/vector-0/{arch}/{name}")
        };
        if let Some(entry) = entries.get(&relative) {
            ensure!(
                is_deb && *entry == entry_value(identity)?,
                "Package already present; reuse signed publication"
            );
        }
        let destination = root.join(&relative);
        copy(&work.join("input").join(name), &destination)?;
        if is_deb {
            incoming.insert(
                arch.to_owned(),
                run(
                    command("apt-ftparchive")
                        .arg("packages")
                        .arg(
                            relative
                                .strip_prefix("apt/")
                                .context("Invalid Debian path")?,
                        )
                        .current_dir(root.join("apt")),
                    None,
                )?,
            );
        } else {
            let cached = work.join("reuse").join(name);
            if cached.exists() {
                copy(&cached, &destination)?;
                lint_package(&destination, identity)?;
                verify_rpm(&signer.rpmdb, &destination)?;
            } else {
                signer.rpm(&destination)?;
            }
        }
    }
    let empty = BTreeMap::new();
    for selected in ["stable", "beta"] {
        if previous.is_none() || selected == channel {
            apt_indexes(
                &root,
                selected,
                if selected == channel {
                    &incoming
                } else {
                    &empty
                },
                &base_files,
                &signer,
            )?;
            for arch in RPM_ARCHES {
                let relative = format!("yum/{selected}/vector-0/{arch}");
                let target = root.join(&relative);
                fs::create_dir_all(&target)?;
                if target.join("repodata").exists() {
                    fs::remove_dir_all(target.join("repodata"))?;
                }
                run(
                    command("createrepo_c")
                        .args([
                            "--no-database",
                            "--checksum",
                            "sha256",
                            "--compress-type",
                            "gz",
                        ])
                        .arg(&target),
                    None,
                )?;
                let old = base_files.join(relative);
                if old.join("repodata/repomd.xml").exists() {
                    let temporary = tempfile::tempdir()?;
                    let uri = |path: &Path| -> Result<String> {
                        Ok(reqwest::Url::from_file_path(path.canonicalize()?)
                            .map_err(|()| anyhow::anyhow!("Invalid file URI"))?
                            .to_string())
                    };
                    run(
                        command("mergerepo_c")
                            .args([
                                "--all",
                                "--omit-baseurl",
                                "--no-database",
                                "--compress-type",
                                "gz",
                                "--repo",
                            ])
                            .arg(uri(&old)?)
                            .arg("--repo")
                            .arg(uri(&target)?)
                            .arg("--outputdir")
                            .arg(temporary.path()),
                        None,
                    )?;
                    fs::remove_dir_all(target.join("repodata"))?;
                    for (name, path) in files(&temporary.path().join("repodata"))? {
                        copy(&path, &target.join("repodata").join(name))?;
                    }
                }
                repomd_locations(&target)?;
                signer.sign(
                    &target.join("repodata/repomd.xml"),
                    false,
                    &target.join("repodata/repomd.xml.asc"),
                )?;
            }
        }
    }
    for (name, path) in files(&root)? {
        let entry = Entry::file(&path)?;
        ensure!(
            mutable(&name) || entries.get(&name).is_none_or(|old| *old == entry),
            "Immutable object changed"
        );
        entries.insert(name, entry);
    }
    let snapshot = Snapshot::new(
        entries,
        previous.map(|s| s.id),
        Some(input_id),
        root.clone(),
    )?;
    verify_metadata(&snapshot, &root, &signer.public)?;
    write(&result.join("manifest.json"), &snapshot.data)?;
    copy(&work.join("input.json"), &result.join("input.json"))?;
    Snapshot::local(&root)?;
    Ok(snapshot.id)
}
