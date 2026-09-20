# Maintainer: GB LMX <GBLMX@users.noreply.github.com>
pkgname=boxpigma-gblmx-bin
_pkgname=boxpigma
pkgver=1.1.0
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
sha256sums=('be7f64dbc6f57f7f859e53cb3de6be9411c81a201022904f11ba5a6310f67857')

package() {
    install -Dm755 "${srcdir}/${_pkgname}" "${pkgdir}/usr/bin/${_pkgname}"

    # install -Dm644 "${srcdir}/LICENSE" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
