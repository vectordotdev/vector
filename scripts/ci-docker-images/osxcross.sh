set -ex

main() {
    local dependencies=(

    )

    apt-get update
    local purge_list=()
    for dep in ${dependencies[@]}; do
        if ! dpkg -L $dep; then
            apt-get install --no-install-recommends -y $dep
            purge_list+=( $dep )
        fi
    done

    # We are in /
    git clone https://github.com/tpoechtrager/osxcross.git
    cd osxcross
    curl -O https://vector-ci-assets.s3.amazonaws.com/MacOSX10.14.sdk.tar.xz
    mv -v MacOSX10.14.sdk.tar.xz tarballs/
    UNATTENDED=yes OSX_VERSION_MIN=10.7 ./build_gcc.sh
    ls /osxcross/target/bin
    cd ..

    # clean up
    apt-get purge --auto-remove -y ${purge_list[@]}
}

main "${@}"
