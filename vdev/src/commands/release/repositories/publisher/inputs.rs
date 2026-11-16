//! Admission of already-built, immutable GitHub release assets.
use super::model::{Config, Entry, canonical, digest, entry_value, files, is_digest, string};
use anyhow::{Context, Result, ensure};
use reqwest::{blocking::Client, redirect::Policy};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

pub(super) const DEB_ARCHES: [&str; 3] = ["amd64", "arm64", "armhf"];
pub(super) const RPM_ARCHES: [&str; 3] = ["x86_64", "aarch64", "armv7hl"];
pub(super) fn versions(tag: &str, channel: &str) -> Result<(String, String, String)> {
    ensure!(
        regex::Regex::new(
            r"^v0\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-([0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*))?$"
        )?
        .is_match(tag),
        "Expected a Vector 0.x release tag"
    );
    ensure!(
        matches!(channel, "stable" | "beta") && (channel != "stable" || !tag.contains('-')),
        "Invalid channel or stable prerelease"
    );
    let raw = tag
        .strip_prefix('v')
        .context("Missing tag prefix")?
        .to_owned();
    let native = raw.replacen('-', "~", 1);
    Ok((
        raw,
        format!("{native}-1"),
        format!("{}-1", native.replace('-', ".")),
    ))
}
pub(super) fn expected_packages(tag: &str, channel: &str) -> Result<BTreeMap<String, Value>> {
    let (_, deb, rpm) = versions(tag, channel)?;
    Ok(DEB_ARCHES
        .into_iter()
        .map(|arch| {
            (
                format!("vector_{deb}_{arch}.deb"),
                json!({"format":"deb","arch":arch,"version":deb}),
            )
        })
        .chain(RPM_ARCHES.into_iter().map(|arch| {
            (
                format!("vector-{rpm}.{arch}.rpm"),
                json!({"format":"rpm","arch":arch,"version":rpm}),
            )
        }))
        .collect())
}
pub(super) fn validate_run(config: &Config, run: &Value, tag: &str, commit: &str) -> Result<()> {
    ensure!(
        run["repository"]["id"] == config.repository_id
            && run["head_repository"]["id"] == config.repository_id,
        "Wrong source repository or fork run"
    );
    ensure!(
        run["path"] == config.workflow && run["event"] == "push" && run["head_branch"] == tag,
        "Source must be the release workflow tag push"
    );
    ensure!(
        run["status"] == "completed" && run["conclusion"] == "success",
        "Source run did not succeed"
    );
    ensure!(
        run["head_sha"] == commit
            && commit.len() == 40
            && commit
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Run and tag commit differ"
    );
    Ok(())
}
pub(super) fn select_assets(
    release: &Value,
    tag: &str,
    channel: &str,
) -> Result<BTreeMap<String, Value>> {
    let (raw, deb, _) = versions(tag, channel)?;
    ensure!(
        release["tag_name"] == tag
            && release["draft"] == false
            && release["published_at"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
        "Release must be published and match tag"
    );
    ensure!(
        channel != "stable" || release["prerelease"] == false,
        "Stable release marked prerelease"
    );
    let expected = expected_packages(tag, channel)?;
    let sums = format!("vector-{raw}-SHA256SUMS");
    let mut seen = BTreeSet::new();
    let mut selected = BTreeMap::new();
    for asset in release["assets"]
        .as_array()
        .context("Missing release assets")?
    {
        let name = string(asset, "name")?;
        ensure!(seen.insert(name), "Duplicate release asset");
        ensure!(
            !name.ends_with(".deb") && !name.ends_with(".rpm")
                || expected.contains_key(name)
                || name == format!("vector_{deb}_armel.deb"),
            "Unexpected Linux package: {name}"
        );
        if !expected.contains_key(name) && name != sums {
            continue;
        }
        let hash = string(asset, "digest")?
            .strip_prefix("sha256:")
            .context("Asset lacks SHA-256 digest")?;
        ensure!(is_digest(hash), "Invalid asset digest");
        let size = asset["size"].as_u64().context("Invalid asset size")?;
        ensure!(
            size > 0 && size < 1024 * 1024 * 1024 && asset["id"].as_u64().is_some_and(|id| id > 0),
            "Invalid asset size or ID"
        );
        let mut item = expected.get(name).cloned().unwrap_or_else(|| json!({}));
        item["id"] = asset["id"].clone();
        item["size"] = json!(size);
        item["sha256"] = json!(hash);
        selected.insert(name.to_owned(), item);
    }
    ensure!(
        selected.len() == expected.len() + 1,
        "Missing required packages or SHA256SUMS"
    );
    Ok(selected)
}
pub(super) fn parse_checksums(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let expression = regex::Regex::new(r"^([0-9a-f]{64}) [ *]([^/\\ \t\r\n\x0b\x0c]+)$")?;
    let mut sums = BTreeMap::new();
    for line in std::str::from_utf8(bytes)?.lines() {
        let captures = expression
            .captures(line)
            .context("Malformed SHA256SUMS entry")?;
        ensure!(
            sums.insert(captures[2].to_owned(), captures[1].to_owned())
                .is_none(),
            "Duplicate SHA256SUMS entry"
        );
    }
    Ok(sums)
}
pub(super) fn client(redirects: bool) -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(90))
        .https_only(true)
        .redirect(if redirects {
            Policy::limited(10)
        } else {
            Policy::none()
        })
        .user_agent("vector-package-publisher")
        .build()?)
}
fn github(client: &Client, endpoint: &str) -> Result<Value> {
    let mut request = client
        .get(format!("https://api.github.com/{endpoint}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Ok(token) = std::env::var("GH_TOKEN") {
        request = request.bearer_auth(token);
    }
    let response = request.send()?.error_for_status()?;
    ensure!(
        response.status().is_success(),
        "GitHub API redirects are forbidden"
    );
    Ok(response.json()?)
}
pub(super) fn download(
    client: &Client,
    url: &str,
    destination: &Path,
    expected: &Entry,
) -> Result<()> {
    ensure!(!destination.is_symlink(), "Invalid download destination");
    let mut response = client.get(url).send()?.error_for_status()?;
    ensure!(
        response.url().scheme() == "https",
        "Download redirected away from HTTPS"
    );
    let mut file = fs::File::create(destination)?;
    let size = std::io::copy(
        &mut Read::by_ref(&mut response).take(expected.size + 1),
        &mut file,
    )?;
    file.flush()?;
    ensure!(
        size == expected.size && Entry::file(destination)? == *expected,
        "Asset digest/size mismatch"
    );
    Ok(())
}
pub(super) fn fetch(
    config: &Config,
    work: &Path,
    run_id: u64,
    tag: &str,
    channel: &str,
) -> Result<()> {
    expected_packages(tag, channel)?;
    let api = client(false)?;
    let public = client(true)?;
    let repo = &config.repository;
    let run = github(&api, &format!("repos/{repo}/actions/runs/{run_id}"))?;
    // Admitted tags contain only URL-path-safe ASCII.
    let mut reference =
        github(&api, &format!("repos/{repo}/git/ref/tags/{tag}"))?["object"].clone();
    for _ in 0..4 {
        if reference["type"] == "commit" {
            break;
        }
        ensure!(reference["type"] == "tag", "Ref does not identify a commit");
        reference = github(
            &api,
            &format!("repos/{repo}/git/tags/{}", string(&reference, "sha")?),
        )?["object"]
            .clone();
    }
    ensure!(reference["type"] == "commit", "Too many tag indirections");
    validate_run(config, &run, tag, string(&reference, "sha")?)?;
    let endpoint = format!("repos/{repo}/releases/tags/{tag}");
    let release = github(&api, &endpoint)?;
    let assets = select_assets(&release, tag, channel)?;
    let manifest = json!({"schema":1,"repository":repo,"repository_id":config.repository_id,"run":run_id,"commit":reference["sha"],"tag":tag,"channel":channel,"release":release["id"],"assets":assets});
    let data = canonical(&manifest)?;
    let destination = work.join("input");
    fs::create_dir_all(&destination)?;
    ensure!(
        files(&destination)?.keys().all(|n| assets.contains_key(n)),
        "Unexpected input files"
    );
    if work.join("input.json").exists() {
        ensure!(
            fs::read(work.join("input.json"))? == data,
            "Frozen inputs changed"
        );
    }
    for (name, asset) in &assets {
        let path = destination.join(name);
        let expected = entry_value(asset)?;
        if path.exists() {
            ensure!(Entry::file(&path)? == expected, "Downloaded asset changed");
            continue;
        }
        let temporary = work.join("download.partial");
        download(
            &public,
            &format!("https://github.com/{repo}/releases/download/{tag}/{name}"),
            &temporary,
            &expected,
        )?;
        fs::rename(temporary, path)?;
    }
    let after = github(&api, &endpoint)?;
    ensure!(
        after["id"] == release["id"] && select_assets(&after, tag, channel)? == assets,
        "Release assets changed during download"
    );
    fs::write(work.join("input.json"), data)?;
    verify_input(work)?;
    println!("Verified six repository packages and published checksums");
    Ok(())
}
pub(super) fn verify_input(work: &Path) -> Result<(Value, String)> {
    let data = fs::read(work.join("input.json"))?;
    let manifest: Value = serde_json::from_slice(&data)?;
    ensure!(
        canonical(&manifest)? == data && manifest["schema"] == 1,
        "Invalid input manifest"
    );
    ensure!(
        manifest["repository"] == "vectordotdev/vector" && manifest["repository_id"] == 146_327_667,
        "Wrong input repository"
    );
    let tag = string(&manifest, "tag")?;
    let expected = expected_packages(tag, string(&manifest, "channel")?)?;
    let sums_name = format!(
        "vector-{}-SHA256SUMS",
        tag.strip_prefix('v').context("Missing tag prefix")?
    );
    let assets = manifest["assets"].as_object().context("Missing assets")?;
    let wanted: BTreeSet<_> = expected.keys().chain(std::iter::once(&sums_name)).collect();
    ensure!(
        assets.keys().collect::<BTreeSet<_>>() == wanted
            && files(&work.join("input"))?.keys().collect::<BTreeSet<_>>() == wanted,
        "Input file set changed"
    );
    for (name, asset) in assets {
        ensure!(
            Entry::file(&work.join("input").join(name))? == entry_value(asset)?,
            "Input asset changed: {name}"
        );
        if let Some(identity) = expected.get(name) {
            for (field, value) in identity.as_object().context("Invalid identity")? {
                ensure!(asset.get(field) == Some(value), "Native identity changed");
            }
        }
    }
    let sums = parse_checksums(&fs::read(work.join("input").join(sums_name))?)?;
    for name in expected.keys() {
        ensure!(
            sums.get(name).map(String::as_str) == assets[name]["sha256"].as_str(),
            "SHA256SUMS disagrees with GitHub digest"
        );
    }
    Ok((manifest, digest(&data)))
}
