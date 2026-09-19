# Empacotamento do photoshow

| Alvo | Como gerar | Status |
|---|---|---|
| `.deb` (Debian/Ubuntu) | `cargo deb` (config em `Cargo.toml`) | ✅ build local verificado |
| `.rpm` (Fedora/openSUSE) | `cargo generate-rpm` | ✅ build local verificado |
| Arch (AUR) | `dist/arch/PKGBUILD` | validar com `makepkg -s` |
| Tarball genérico | CI (`release.yml`) | via tag `v*` |
| Windows `.exe` | CI (`release.yml`, `windows-latest`) | via tag `v*` |
| Flatpak | `dist/flatpak/` | manifesto pronto, falta vendor |

## AUR

```bash
cd dist/arch
# após a tag existir, preencha o sha256 real:
# makepkg -g >> PKGBUILD  (e regenere o .SRCINFO)
makepkg -s
```

Submissão ao AUR é manual (conta + ssh no `aur.archlinux.org`):

```bash
git clone ssh://aur@aur.archlinux.org/photoshow.git
cp PKGBUILD .SRCINFO photoshow/
git add PKGBUILD .SRCINFO && git commit -m ... && git push
```

## Flatpak

Pré-requisitos: `flatpak-builder` + SDK Freedesktop 24.08.

```bash
# 1. Venda as crates (Flathub não tem rede no build):
python3 flatpak-cargo-generator.py Cargo.lock -o dist/flatpak/cargo-sources.json
# 2. Build local:
flatpak-builder --force-clean build-dir dist/flatpak/io.github.raillen.photoshow.yml
flatpak-builder --run build-dir dist/flatpak/io.github.raillen.photoshow.yml photoshow
```

Submissão ao Flathub segue o guia deles (fork `flathub/flathub`,
PR com este manifesto + `cargo-sources.json` commitado).
