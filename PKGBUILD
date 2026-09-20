# Maintainer: GB LMX <GBLMX@users.noreply.github.com>
pkgname=boxpigma-gblmx-bin
_pkgname=boxpigma
pkgver=1.0.0.gblmx.1
pkgrel=1
pkgdesc="A netease cloud music client (GBLMX fork build)"
arch=('x86_64')
url="https://github.com/GBLMX/pigma"
license=('Apache-2.0')
# Upstream's `boxpigma-bin` also provides `boxpigma`, so these entries cover it too: both
# packages install /usr/bin/boxpigma and cannot be installed side by side.
provides=("${_pkgname}")
conflicts=("${_pkgname}")
source=("${_pkgname}-${pkgver}.tar.gz::${url}/releases/download/v${pkgver}/${_pkgname}-x86_64-unknown-linux-gnu.tar.gz")
# Checksum of the release asset referenced above; the release workflow recomputes it from
# the published asset on every tag, so it cannot go stale in the AUR package.
sha256sums=('a724ed0e03b7ef37616491360c003538197417ca543a69c799ffcafd2c6800e5')

package() {
    install -Dm755 "${srcdir}/${_pkgname}" "${pkgdir}/usr/bin/${_pkgname}"

    # install -Dm644 "${srcdir}/LICENSE" "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
