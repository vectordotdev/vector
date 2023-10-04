#!/bin/bash

# setup.sh
#
# A script intended to setup DEB and RPM package repositories for Vector. This
# also provides an opt-in option to remove the old cloudsmith based repositories.

# Packages at https://apt.vector.dev will be signed with the following key(s)
# once we have stopped publishing packages to cloudsmith repositories.
# ------------------------------------------------------------------------------
# DATADOG_APT_KEY_CURRENT.public always contains key used to sign current
# repodata and newly released packages
# DATADOG_APT_KEY_F14F620E.public expires in 2032
# DATADOG_APT_KEY_C0962C7D.public expires in 2028
APT_GPG_KEYS=("DATADOG_APT_KEY_CURRENT.public" "DATADOG_APT_KEY_C0962C7D.public" "DATADOG_APT_KEY_F14F620E.public")

# Packages at https://yum.vector.dev have been re-signed with the following
# key(s). While the packages are identical, the checksums will not match for
# the equivalent packages in the new (https://yum.vector.dev) and old
# (https://repositories.timber.io/public/vector) repositories.
# ------------------------------------------------------------------------------
# DATADOG_RPM_KEY_CURRENT.public always contains key used to sign current
# repodata and newly released packages
# DATADOG_RPM_KEY_FD4BF915.public expires in 2024
# DATADOG_RPM_KEY_B01082D3.public expires in 2028
RPM_GPG_KEYS=("DATADOG_RPM_KEY_CURRENT.public" "DATADOG_RPM_KEY_B01082D3.public" "DATADOG_RPM_KEY_FD4BF915.public")

echo -e "\033[34m\n* Vector repository setup script\n\033[0m"

if [ -n "$CSM_MIGRATE" ]; then
    cloudsmith_migrate=$CSM_MIGRATE
else
    cloudsmith_migrate=false
fi

if [ -n "$VEC_REPO_URL" ]; then
    repository_url=$VEC_REPO_URL
else
    repository_url="vector.dev"
fi

if [ -n "$TESTING_KEYS_URL" ]; then
  keys_url=$TESTING_KEYS_URL
else
  keys_url="keys.datadoghq.com"
fi

if [ -n "$TESTING_YUM_URL" ]; then
  yum_url=$TESTING_YUM_URL
else
  yum_url="yum.${repository_url}"
fi

# We turn off `repo_gpgcheck` for custom REPO_URL, unless explicitly turned
# on via VEC_RPM_REPO_GPGCHECK.
# There is more logic for redhat/suse in their specific code branches below
rpm_repo_gpgcheck=
if [ -n "$VEC_RPM_REPO_GPGCHECK" ]; then
    rpm_repo_gpgcheck=$VEC_RPM_REPO_GPGCHECK
else
    if [ -n "$REPO_URL" ] || [ -n "$VEC_REPO_URL" ]; then
        rpm_repo_gpgcheck=0
    fi
fi

if [ -n "$TESTING_APT_URL" ]; then
  apt_url=$TESTING_APT_URL
else
  apt_url="apt.${repository_url}"
fi

vector_major_version=0
if [ -n "$VECTOR_MAJOR_VERSION" ]; then
  vector_major_version=$VECTOR_MAJOR_VERSION
fi

vector_dist_channel=stable
if [ -n "$VECTOR_DIST_CHANNEL" ]; then
  vector_dist_channel=$VECTOR_DIST_CHANNEL
fi

if [ -n "$TESTING_YUM_VERSION_PATH" ]; then
  yum_version_path=$TESTING_YUM_VERSION_PATH
else
  yum_version_path="${vector_dist_channel}/vector-${vector_major_version}"
fi

if [ -n "$TESTING_APT_REPO_VERSION" ]; then
  apt_repo_version=$TESTING_APT_REPO_VERSION
else
  apt_repo_version="${vector_dist_channel} vector-${vector_major_version}"
fi

# OS/Distro Detection
# Try lsb_release, fallback with /etc/issue then uname command
KNOWN_DISTRIBUTION="(Debian|Ubuntu|RedHat|CentOS|AmazonRocky|AlmaLinux)"
DISTRIBUTION=$(lsb_release -d 2>/dev/null | grep -Eo $KNOWN_DISTRIBUTION  || grep -Eo $KNOWN_DISTRIBUTION /etc/issue 2>/dev/null || grep -Eo $KNOWN_DISTRIBUTION /etc/Eos-release 2>/dev/null || grep -m1 -Eo $KNOWN_DISTRIBUTION /etc/os-release 2>/dev/null || uname -s)

if [ "$DISTRIBUTION" == "Darwin" ]; then
    echo -e "\033[31mThis script does not support installing on macOS\033[0m"
    exit 1;

elif [ -f /etc/debian_version ] || [ "$DISTRIBUTION" == "Debian" ] || [ "$DISTRIBUTION" == "Ubuntu" ]; then
    OS="Debian"
elif [ -f /etc/redhat-release ] || [ "$DISTRIBUTION" == "RedHat" ] || [ "$DISTRIBUTION" == "CentOS" ] || [ "$DISTRIBUTION" == "Amazon" ] || [ "$DISTRIBUTION" == "Rocky" ] || [ "$DISTRIBUTION" == "AlmaLinux" ]; then
    OS="RedHat"
# Some newer distros like Amazon may not have a redhat-release file
elif [ -f /etc/system-release ] || [ "$DISTRIBUTION" == "Amazon" ]; then
    OS="RedHat"
fi

# Root user detection
if [ "$UID" == "0" ]; then
    sudo_cmd=''
else
    sudo_cmd='sudo'
fi

# Install the necessary package sources
if [ "$OS" == "RedHat" ]; then
    echo -e "\033[34m\n* Installing YUM sources for Vector\n\033[0m"

    if "$cloudsmith_migrate"; then
        rm "/etc/yum.repos.d/timber-vector.repo"
    fi

    # Because of https://bugzilla.redhat.com/show_bug.cgi?id=1792506, we disable
    # repo_gpgcheck on RHEL/CentOS 8.1
    if [ -z "$rpm_repo_gpgcheck" ]; then
        if grep -q "8\.1\(\b\|\.\)" /etc/redhat-release 2>/dev/null; then
            rpm_repo_gpgcheck=0
        else
            rpm_repo_gpgcheck=1
        fi
    fi

    gpgkeys=''
    separator='\n       '
    for key_path in "${RPM_GPG_KEYS[@]}"; do
      gpgkeys="${gpgkeys:+"${gpgkeys}${separator}"}https://${keys_url}/${key_path}"
    done

    $sudo_cmd sh -c "echo -e '[vector]\nname = Vector\nbaseurl = https://${yum_url}/${yum_version_path}/\$basearch/\nenabled=1\ngpgcheck=1\nrepo_gpgcheck=${rpm_repo_gpgcheck}\npriority=1\ngpgkey=${gpgkeys}' > /etc/yum.repos.d/vector.repo"

    $sudo_cmd yum -y clean metadata
elif [ "$OS" == "Debian" ]; then
    if "$cloudsmith_migrate"; then
        rm "/etc/apt/sources.list.d/timber-vector.list"
    fi

    apt_trusted_d_keyring="/etc/apt/trusted.gpg.d/datadog-archive-keyring.gpg"
    apt_usr_share_keyring="/usr/share/keyrings/datadog-archive-keyring.gpg"

    VEC_APT_INSTALL_ERROR_MSG=/tmp/vector_install_error_msg
    MAX_RETRY_NB=10
    for i in $(seq 1 $MAX_RETRY_NB)
    do
        printf "\033[34m\n* Installing apt-transport-https, curl and gnupg\n\033[0m\n"
        $sudo_cmd apt-get update || printf "\033[31m'apt-get update' failed, the script will not install the latest version of apt-transport-https.\033[0m\n"
        # installing curl might trigger install of additional version of libssl; this will fail the installation process,
        # see https://unix.stackexchange.com/q/146283 for reference - we use DEBIAN_FRONTEND=noninteractive to fix that
        apt_exit_code=0
        if [ -z "$sudo_cmd" ]; then
            # if $sudo_cmd is empty, doing `$sudo_cmd X=Y command` fails with
            # `X=Y: command not found`; therefore we don't prefix the command with
            # $sudo_cmd at all in this case
            DEBIAN_FRONTEND=noninteractive apt-get install -y apt-transport-https curl gnupg 2>$VEC_APT_INSTALL_ERROR_MSG  || apt_exit_code=$?
        else
            $sudo_cmd DEBIAN_FRONTEND=noninteractive apt-get install -y apt-transport-https curl gnupg 2>$VEC_APT_INSTALL_ERROR_MSG || apt_exit_code=$?
        fi

        if grep "Could not get lock" $VEC_APT_INSTALL_ERROR_MSG; then
            RETRY_TIME=$((i*5))
            printf "\033[31mInstallation failed: Unable to get lock.\nRetrying in ${RETRY_TIME}s ($i/$MAX_RETRY_NB)\033[0m\n"
            sleep $RETRY_TIME
        elif [ $apt_exit_code -ne 0 ]; then
            cat $VEC_APT_INSTALL_ERROR_MSG
            exit $apt_exit_code
        else
            break
        fi
    done

    printf "\033[34m\n* Installing APT package sources for Vector\n\033[0m\n"
    $sudo_cmd sh -c "echo 'deb [signed-by=${apt_usr_share_keyring}] https://${apt_url}/ ${apt_repo_version}' > /etc/apt/sources.list.d/vector.list"
    $sudo_cmd sh -c "chmod a+r /etc/apt/sources.list.d/vector.list"

    if [ ! -f $apt_usr_share_keyring ]; then
        $sudo_cmd touch $apt_usr_share_keyring
    fi
    # ensure that the _apt user used on Ubuntu/Debian systems to read GPG keyrings
    # can read our keyring
    $sudo_cmd chmod a+r $apt_usr_share_keyring

    for key in "${APT_GPG_KEYS[@]}"; do
        $sudo_cmd curl --retry 5 -o "/tmp/${key}" "https://${keys_url}/${key}"
        $sudo_cmd cat "/tmp/${key}" | $sudo_cmd gpg --import --batch --no-default-keyring --keyring "$apt_usr_share_keyring"
    done

    release_version="$(grep VERSION_ID /etc/os-release | cut -d = -f 2 | xargs echo | cut -d "." -f 1)"
    if { [ "$DISTRIBUTION" == "Debian" ] && [ "$release_version" -lt 9 ]; } || \
       { [ "$DISTRIBUTION" == "Ubuntu" ] && [ "$release_version" -lt 16 ]; }; then
        # copy with -a to preserve file permissions
        $sudo_cmd cp -a $apt_usr_share_keyring $apt_trusted_d_keyring
    fi

    $sudo_cmd apt-get update -o Dir::Etc::sourcelist="sources.list.d/vector.list" -o Dir::Etc::sourceparts="-" -o APT::Get::List-Cleanup="0"
else
    printf "\033[31mThis combination of distribution and architecture doesn't appear to be supported, please open a GitHub issue\033[0m\n"
    exit 1
fi

echo -e "\033[34m\n* Vector repository has been setup\n\033[0m"