//! Descoberta de arquivos de imagem: pastas e arquivos soltos.
//!
//! Tipos de domínio (anti-primitivo): [`PhotoPath`] em vez de `String` solta.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};

/// Extensões suportadas no MVP (minúsculas, sem ponto).
pub const SUPPORTED_EXTENSIONS: &[&str] =
    &["jpg", "jpeg", "png", "webp", "tiff", "tif", "bmp", "gif"];

/// Dados imutáveis compartilhados por todas as views da mesma foto.
#[derive(Debug, PartialEq, Eq, Hash)]
struct PhotoPathData {
    path: PathBuf,
    display_name: String,
    sort_key: String,
}

/// Handle barato e validado de uma foto.
///
/// Clonar este tipo clona apenas o `Arc`; `photos` e `visible` não duplicam
/// buffers de caminho/nome para coleções grandes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PhotoPath(Arc<PhotoPathData>);

impl PhotoPath {
    /// Constrói a partir de qualquer caminho; aceita só extensões suportadas.
    pub fn new(path: PathBuf) -> Option<Self> {
        if !has_supported_extension(&path) {
            return None;
        }
        let display_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let sort_key = display_name.to_lowercase();
        Some(Self(Arc::new(PhotoPathData {
            path,
            display_name,
            sort_key,
        })))
    }

    /// Caminho interno.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0.path
    }

    /// Nome do arquivo para exibição.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.0.display_name.clone()
    }

    /// Chave case-insensitive pré-computada usada para ordenação.
    #[must_use]
    pub fn sort_key(&self) -> &str {
        &self.0.sort_key
    }
}

/// Verifica extensão (case-insensitive).
#[must_use]
pub fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Varredura em background: walk + filtro + ordenação fora da thread da UI.
///
/// O resultado chega pelo channel com o `id` da geração — se o usuário abrir
/// outra pasta antes de terminar, o resultado obsoleto é descartado pelo app.
/// Usa o crate `ignore`: pula `.git` sempre, ocultas e `gitignore` conforme
/// as opções (rápido em árvores com milhares de arquivos não-imagem).
#[derive(Debug, Clone, Default)]
pub struct ScanController {
    generation: Arc<AtomicU64>,
}

impl ScanController {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a scan and invalidates every older scan owned by this controller.
    ///
    /// Old workers observe the generation while walking and stop before doing
    /// the rest of the filesystem IO. They do not emit a result.
    pub fn scan(
        &self,
        dir: PathBuf,
        opts: ScanOptions,
        id: u64,
    ) -> mpsc::Receiver<ScanResult> {
        self.generation.store(id, Ordering::Release);
        let active_generation = Arc::clone(&self.generation);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let Some((photos, files_seen, errors_seen, sample_errors)) =
                walk_photos(&dir, opts, &active_generation, id)
            else {
                return;
            };
            let _ = tx.send(ScanResult {
                id,
                dir,
                photos,
                files_seen,
                errors_seen,
                sample_errors,
            });
        });
        rx
    }
}

/// Convenience scanner for isolated callers/tests. Application code should keep
/// one ScanController so newer requests can cancel older walks.
pub fn scan_dir_async(dir: PathBuf, opts: ScanOptions, id: u64) -> mpsc::Receiver<ScanResult> {
    ScanController::new().scan(dir, opts, id)
}

/// Opções da varredura (espelham as preferências do menu ⚙).
#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    /// Respeita `.gitignore`/`.ignore` (vale fora de repo git também).
    pub respect_gitignore: bool,
    /// Pula arquivos e pastas ocultas (dotfiles).
    pub skip_hidden: bool,
}

/// Resultado de uma varredura (lista vazia = nenhuma imagem, não erro).
pub struct ScanResult {
    /// Geração do pedido (para descartar obsoletos).
    pub id: u64,
    /// Pasta varrida.
    pub dir: PathBuf,
    /// Fotos ordenadas por nome.
    pub photos: Vec<PhotoPath>,
    /// Arquivos inspecionados (para o status "N verificados").
    pub files_seen: u64,
    /// Entradas que falharam por IO/permissão durante a caminhada.
    pub errors_seen: u64,
    /// Pequena amostra para diagnóstico sem acumular mensagens de árvores enormes.
    pub sample_errors: Vec<String>,
}

fn is_skipped_dir(entry: &ignore::DirEntry) -> bool {
    entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
        && entry.file_name().to_string_lossy() == ".git"
}

fn walk_photos(
    dir: &Path,
    opts: ScanOptions,
    active_generation: &AtomicU64,
    id: u64,
) -> Option<(Vec<PhotoPath>, u64, u64, Vec<String>)> {
    const ERROR_SAMPLE_CAP: usize = 5;

    let mut builder = ignore::WalkBuilder::new(dir);
    builder
        .hidden(opts.skip_hidden)
        .git_ignore(opts.respect_gitignore)
        .git_global(opts.respect_gitignore)
        .git_exclude(opts.respect_gitignore)
        .parents(opts.respect_gitignore)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|e| !is_skipped_dir(e));

    let mut photos = Vec::new();
    let mut files_seen = 0u64;
    let mut errors_seen = 0u64;
    let mut sample_errors = Vec::new();

    for result in builder.build() {
        if active_generation.load(Ordering::Acquire) != id {
            return None;
        }
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                errors_seen += 1;
                if sample_errors.len() < ERROR_SAMPLE_CAP {
                    sample_errors.push(error.to_string());
                }
                continue;
            }
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        files_seen += 1;
        if let Some(photo) = PhotoPath::new(entry.path().to_path_buf()) {
            photos.push(photo);
        }
    }

    photos.sort_by(|left, right| left.sort_key().cmp(right.sort_key()));
    Some((photos, files_seen, errors_seen, sample_errors))
}

/// Filtra uma lista solta de arquivos (diálogo rfd) para fotos válidas.
#[must_use]
pub fn filter_loose_files(paths: Vec<PathBuf>) -> Vec<PhotoPath> {
    let mut photos: Vec<PhotoPath> = paths.into_iter().filter_map(PhotoPath::new).collect();
    photos.sort_by(|left, right| left.sort_key().cmp(right.sort_key()));
    photos
}

/// Subpastas diretas ordenadas por nome (ignora erros de permissão).
#[must_use]
pub fn list_subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort_by_key(|a| dir_name(a).to_lowercase());
    dirs
}

fn dir_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// Pasta/arquivo oculto (nome começa com ponto)?
#[must_use]
pub fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().starts_with('.'))
        .unwrap_or(false)
}

/// Renomeia uma foto dentro da mesma pasta. Erro em texto para a status bar.
pub fn rename_photo(path: &Path, new_name: &str) -> Result<PathBuf, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err(String::from("Nome vazio."));
    }
    if new_name.contains(['/', '\\']) {
        return Err(String::from("Nome não pode conter barras."));
    }
    let Some(parent) = path.parent() else {
        return Err(String::from("Pasta de origem inválida."));
    };
    let dest = parent.join(new_name);
    if dest == path {
        return Err(String::from("Nome igual ao atual."));
    }
    if dest.exists() {
        return Err(String::from("Já existe um arquivo com esse nome."));
    }
    std::fs::rename(path, &dest).map_err(|e| format!("Falha ao renomear: {e}"))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn accepts_supported_extensions_case_insensitive() {
        assert!(has_supported_extension(Path::new("foto.JPG")));
        assert!(has_supported_extension(Path::new("img.WebP")));
        assert!(!has_supported_extension(Path::new("doc.pdf")));
        assert!(!has_supported_extension(Path::new("sem_extensao")));
    }

    fn scan_opts() -> ScanOptions {
        ScanOptions {
            respect_gitignore: true,
            skip_hidden: true,
        }
    }

    fn recv_scan(rx: mpsc::Receiver<ScanResult>) -> ScanResult {
        rx.recv_timeout(std::time::Duration::from_secs(30))
            .expect("scan termina")
    }

    #[test]
    fn scan_async_lists_and_sorts_photos() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["b.png", "a.JPG", "nota.txt", "c.gif"] {
            fs::write(dir.path().join(name), b"x").expect("write");
        }
        let res = recv_scan(scan_dir_async(dir.path().to_path_buf(), scan_opts(), 7));
        assert_eq!(res.id, 7);
        assert_eq!(res.files_seen, 4);
        let names: Vec<_> = res.photos.iter().map(|p| p.display_name()).collect();
        assert_eq!(names, vec!["a.JPG", "b.png", "c.gif"]);
    }

    #[test]
    fn scan_async_empty_dir_returns_empty_vec() {
        let dir = tempfile::tempdir().expect("tempdir");
        let res = recv_scan(scan_dir_async(dir.path().to_path_buf(), scan_opts(), 1));
        assert!(res.photos.is_empty());
    }

    #[test]
    fn scan_async_respects_gitignore_and_hidden() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).expect("mkdir");
        fs::write(sub.join(".gitignore"), b"*.png\n").expect("write");
        fs::write(sub.join("a.png"), b"x").expect("write");
        fs::write(sub.join("b.jpg"), b"x").expect("write");
        fs::create_dir(sub.join(".hidden")).expect("mkdir");
        fs::write(sub.join(".hidden").join("c.jpg"), b"x").expect("write");

        let res = recv_scan(scan_dir_async(dir.path().to_path_buf(), scan_opts(), 1));
        let names: Vec<_> = res.photos.iter().map(|p| p.display_name()).collect();
        assert_eq!(names, vec!["b.jpg"]);

        let no_opts = ScanOptions {
            respect_gitignore: false,
            skip_hidden: false,
        };
        let res = recv_scan(scan_dir_async(dir.path().to_path_buf(), no_opts, 2));
        let names: Vec<_> = res.photos.iter().map(|p| p.display_name()).collect();
        assert_eq!(names, vec!["a.png", "b.jpg", "c.jpg"]);
    }

    #[test]
    fn scan_async_always_skips_git_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let git = dir.path().join(".git");
        fs::create_dir(&git).expect("mkdir");
        fs::write(git.join("x.jpg"), b"x").expect("write");
        let no_opts = ScanOptions {
            respect_gitignore: false,
            skip_hidden: false,
        };
        let res = recv_scan(scan_dir_async(dir.path().to_path_buf(), no_opts, 1));
        assert!(res.photos.is_empty());
    }

    #[test]
    fn filter_loose_files_drops_unsupported() {
        let photos = filter_loose_files(vec![
            PathBuf::from("/x/foto.png"),
            PathBuf::from("/x/texto.txt"),
        ]);
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].display_name(), "foto.png");
    }

    #[test]
    fn list_subdirs_returns_sorted_dirs_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("b_sub")).expect("mkdir");
        fs::create_dir(dir.path().join("a_sub")).expect("mkdir");
        fs::write(dir.path().join("foto.png"), b"x").expect("write");
        let subs = list_subdirs(dir.path());
        let names: Vec<_> = subs
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a_sub", "b_sub"]);
    }

    #[test]
    fn rename_moves_file_and_rejects_conflicts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("foto.png");
        fs::write(&src, b"x").expect("write");
        let dest = rename_photo(&src, "nova.png").expect("rename");
        assert!(dest.exists());
        assert!(!src.exists());
        fs::write(dir.path().join("outra.png"), b"y").expect("write");
        assert!(rename_photo(&dest, "outra.png").is_err());
        assert!(rename_photo(&dest, "").is_err());
        assert!(rename_photo(&dest, "a/b.png").is_err());
    }

    #[test]
    fn hidden_detection_uses_dot_prefix() {
        assert!(is_hidden(Path::new("/a/.config")));
        assert!(!is_hidden(Path::new("/a/fotos")));
    }
}
