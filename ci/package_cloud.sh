OS=$1
VERSION=$2

package_cloud push timberio/vector/${OS} target/debian/vector_${VERSION}_amd64.deb
