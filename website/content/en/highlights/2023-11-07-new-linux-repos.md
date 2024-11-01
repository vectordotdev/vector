---
date: "2023-11-07"
title: "A New Home for Linux Packages"
description: ""
authors: ["spencergilbert"]
pr_numbers: []
release: "0.34.0"
hide_on_release_notes: false
badges:
  type: "announcement"
---

Starting with Vector's `0.34.0` release, the `deb` and `rpm` packages will have
a new home at `vector.dev` courtesy of Datadog.

We will continue to maintain the existing `timber.io` hosting  until **February
28th**, but we **will not** publish future minor release versions to them
starting with the next release, `0.35.0`. Any patch releases for `0.34.0` will
be published to the existing repositories, as well as the new ones.

{{< warning >}}
On February 28th, the `timber.io` package repositories will be decommissioned.
All future minor releases will only be published to the new `vector.dev` package
repositories and not to the `timber.io` repositories.
{{< /warning >}}

We aim to make this a seamless transition by providing time for you to switch
repositories, as well as by publishing previous versions of Vector with the new
repository as a drop-in replacement, so you don't have to update your Vector
version at the same time.

If you have any questions or concerns don't hesitate to reach out on [Discord]
or [Discussions]!

## Migration guide

The following command **removes** the existing repository and configures the
new repository.

```sh
CSM_MIGRATE=true bash -c "$(curl -L https://setup.vector.dev)"
```

Alternatively, `CSM_MIGRATE` may be left unset to leave the removal of the
existing repository to your discretion.

### Manual step-by-step instructions

<details>
  <summary>APT</summary>
1. Remove the existing repository:

```sh
rm "/etc/apt/sources.list.d/timber-vector.list"
```

2. Run the following commands to set up APT to download through HTTPS:

```sh
sudo apt-get update
sudo apt-get install apt-transport-https curl gnupg
```

3. Run the following commands to set up the Vector `deb` repo on your system
and create a Datadog archive keyring:

```sh
echo "deb [signed-by=/usr/share/keyrings/datadog-archive-keyring.gpg] https://apt.vector.dev/ stable vector-0" | sudo tee "/etc/apt/sources.list.d/vector.list"
sudo touch /usr/share/keyrings/datadog-archive-keyring.gpg
sudo chmod a+r /usr/share/keyrings/datadog-archive-keyring.gpg
curl https://keys.datadoghq.com/DATADOG_APT_KEY_CURRENT.public | sudo gpg --no-default-keyring --keyring /usr/share/keyrings/datadog-archive-keyring.gpg --import --batch
curl https://keys.datadoghq.com/DATADOG_APT_KEY_F14F620E.public | sudo gpg --no-default-keyring --keyring /usr/share/keyrings/datadog-archive-keyring.gpg --import --batch
curl https://keys.datadoghq.com/DATADOG_APT_KEY_C0962C7D.public | sudo gpg --no-default-keyring --keyring /usr/share/keyrings/datadog-archive-keyring.gpg --import --batch
```

4. Run the following commands to update your local `apt` repo and install Vector:

```sh
sudo apt-get update
sudo apt-get install vector
```

</details>

<details>
  <summary>RPM</summary>

1. Remove the existing repository:

```sh
rm "/etc/yum.repos.d/timber-vector.repo"
```

2. Run the following commands to set up the Vector `rpm` repo on your system:

```sh
cat <<EOF > /etc/yum.repos.d/vector.repo
[vector]
name = Vector
baseurl = https://yum.vector.dev/stable/vector-0/\$basearch/
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=https://keys.datadoghq.com/DATADOG_RPM_KEY_CURRENT.public
       https://keys.datadoghq.com/DATADOG_RPM_KEY_B01082D3.public
       https://keys.datadoghq.com/DATADOG_RPM_KEY_FD4BF915.public
EOF
```

**Note:** If you are running RHEL 8.1 or CentOS 8.1, use `repo_gpgcheck=0` instead of `repo_gpgcheck=1` in the configuration above.

3. Update your packages and install Vector:

```sh
sudo yum makecache
sudo yum install vector
```

</details>

### Notes

* While the existing packages were migrated without rebuilding them, the RPM
packages _were_ re-signed with a Datadog GPG key. This will cause checksums
to not match between equivalent packages from `vector.dev` and `timber.io`.
* Once packages are released only to `vector.dev`, APT packages will be signed
by a Datadog GPG key. This update will be announced in advance.

[Discord]: https://chat.vector.dev/
[Discussions]: https://discussions.vector.dev/
