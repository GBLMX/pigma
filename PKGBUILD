# Maintainer: GB LMX <GBLMX@users.noreply.github.com>
pkgname=pigma-gblmx-bin
_pkgname=pigma
pkgver=0.2.14.gblmx.8
pkgrel=1
pkgdesc="A netease cloud music client (GBLMX fork build)"
arch=('x86_64')
url="https://github.com/GBLMX/pigma"
license=('Apache-2.0')
# Upstream's `pigma-bin` also provides `pigma`, so these entries cover it too: both
# packages install /usr/bin/pigma and cannot be installed side by side.
provides=("${_pkgname}")
conflicts=("${_pkgname}")
source=("${_pkgname}-${pkgver}.tar.gz::${url}/releases/download/v${pkgver}/${_pkgname}-x86_64-unknown-linux-gnu.tar.gz")
# Checksum of the release asset referenced above; the release workflow recomputes it from
# the published asset on every tag, so it cannot go stale in the AUR package.
sha256sums=('3d217b6013e74b283ad3df48a5bbc4a8ca2937f3328af24d0fa20e03a5a626b4')

package() {
    install -Dm755 "${srcdir}/${_pkgname}" "${pkgdir}/usr/bin/${_pkgname}"

    # install -Dm644 "${srcdir}/LICENSE" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
