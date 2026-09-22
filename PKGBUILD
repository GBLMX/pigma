# Maintainer: GB LMX <GBLMX@users.noreply.github.com>
pkgname=boxpigma-gblmx-bin
_pkgname=boxpigma
pkgver=1.6.0
pkgrel=1
pkgdesc="A netease cloud music client (GBLMX fork build)"
arch=('x86_64')
url="https://github.com/GBLMX/pigma"
license=('Apache-2.0')
# The binary is installed as /usr/bin/boxpigma (upstream's name), so this declares what it
# really provides and conflicts with anything else that owns it. Note that upstream has no AUR
# package: `boxpigma-bin` does not exist there.
provides=("${_pkgname}")
conflicts=("${_pkgname}")
source=("${_pkgname}-${pkgver}.tar.gz::${url}/releases/download/v${pkgver}/${_pkgname}-x86_64-unknown-linux-gnu.tar.gz")
# Checksum of the release asset referenced above. Nothing computes this automatically: bump it
# together with `pkgver` (`curl -sSL <the asset> | sha256sum`), or `makepkg` will refuse.
sha256sums=('8a0fb95e4a3f4b7456e4dce77cb7fe235e7c6c5fd0882eb12c719264482c1170')

package() {
    install -Dm755 "${srcdir}/${_pkgname}" "${pkgdir}/usr/bin/${_pkgname}"

    # install -Dm644 "${srcdir}/LICENSE" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
