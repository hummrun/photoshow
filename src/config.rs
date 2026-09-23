//! Configuração persistente: favoritas, última pasta e preferências.
//!
//! JSON em `<config_dir>/photoshow/config.json`. Tudo best-effort:
//! falha de IO nunca quebra o app, só volta aos padrões.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Preferências editáveis no menu ⚙ Config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Versão do formato persistido. Configs antigas sem o campo migram para 1.
    #[serde(default = "default_config_version")]
    pub version: u32,
    /// Pastas fixadas (navegação rápida).
    #[serde(default)]
    pub favorites: Vec<PathBuf>,
    /// Última pasta aberta (auto-abrir ao iniciar).
    #[serde(default)]
    pub last_folder: Option<PathBuf>,
    /// Pedir confirmação ao sobrescrever o original.
    #[serde(default = "default_true")]
    pub confirm_overwrite: bool,
    /// Qualidade JPEG ao salvar (1..=100).
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u8,
    /// Exibir a faixa de thumbnails.
    #[serde(default = "default_true")]
    pub show_filmstrip: bool,
    /// Reabrir a última pasta ao iniciar.
    #[serde(default = "default_true")]
    pub open_last_on_startup: bool,
    /// Respeita `.gitignore`/`.ignore` na varredura (rápido em repos).
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    /// Pula arquivos e pastas ocultas na varredura.
    #[serde(default = "default_true")]
    pub skip_hidden: bool,
    /// Orçamento de RAM decodificada para prefetch em MB (0 = desativa).
    #[serde(default = "default_prefetch_mb")]
    pub prefetch_max_mb: u64,
    /// Exibe pastas ocultas (dotfiles) na árvore de navegação.
    #[serde(default)]
    pub show_hidden_folders: bool,
    /// Tema visual (slate, charcoal, frost, paper).
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Lado do thumbnail da galeria em px (48..=192).
    #[serde(default = "default_thumb_size")]
    pub thumb_size: f32,
}

fn default_config_version() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

fn default_jpeg_quality() -> u8 {
    90
}

fn default_prefetch_mb() -> u64 {
    64
}

fn default_theme() -> String {
    String::from("slate")
}

fn default_thumb_size() -> f32 {
    88.0
}

/// Temas visuais disponíveis (egui-elegance).
pub const THEMES: &[&str] = &["slate", "charcoal", "frost", "paper"];

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: default_config_version(),
            favorites: Vec::new(),
            last_folder: None,
            confirm_overwrite: true,
            jpeg_quality: 90,
            show_filmstrip: true,
            open_last_on_startup: true,
            respect_gitignore: true,
            skip_hidden: true,
            prefetch_max_mb: 64,
            show_hidden_folders: false,
            theme: String::from("slate"),
            thumb_size: 88.0,
        }
    }
}

impl AppConfig {
    /// Caminho do arquivo de config (None se o SO não informar).
    #[must_use]
    pub fn file_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "photoshow", "photoshow")
            .map(|d| d.config_dir().join("config.json"))
    }

    /// Carrega do disco ou padrões.
    #[must_use]
    pub fn load() -> Self {
        Self::file_path()
            .and_then(|p| Self::load_from(&p))
            .unwrap_or_default()
    }

    /// Carrega de um caminho explícito (usado em testes).
    #[must_use]
    pub fn load_from(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let mut cfg: Self = serde_json::from_str(&text).ok()?;
        if cfg.version == 0 {
            cfg.version = default_config_version();
        }
        cfg.jpeg_quality = cfg.jpeg_quality.clamp(1, 100);
        cfg.prefetch_max_mb = cfg.prefetch_max_mb.min(1024);
        cfg.thumb_size = cfg.thumb_size.clamp(48.0, 192.0);
        // Remove favoritas que não existem mais.
        cfg.favorites.retain(|p| p.is_dir());
        Some(cfg)
    }

    /// Persiste (cria o diretório se preciso). Erro só vira status na UI.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::file_path().ok_or_else(|| String::from("sem diretório de config"))?;
        self.save_to(&path)
    }

    /// Persiste num caminho explícito (usado em testes).
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::atomic_file::write_bytes_atomic(path, text.as_bytes())
    }

    /// Alterna favorito; `true` se fixou, `false` se desafixou.
    pub fn toggle_favorite(&mut self, dir: &Path) -> bool {
        if let Some(i) = self.favorites.iter().position(|f| f == dir) {
            self.favorites.remove(i);
            false
        } else {
            self.favorites.push(dir.to_path_buf());
            true
        }
    }

    /// A pasta está fixada?
    #[must_use]
    pub fn is_favorite(&self, dir: &Path) -> bool {
        self.favorites.iter().any(|f| f == dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        let favorite = dir.path().join("favorita");
        std::fs::create_dir(&favorite).expect("mkdir favorite");

        let mut cfg = AppConfig::default();
        cfg.favorites.push(favorite.clone());
        cfg.jpeg_quality = 77;
        cfg.confirm_overwrite = false;
        cfg.save_to(&path).expect("save");
        let back = AppConfig::load_from(&path).expect("load");
        assert_eq!(back.jpeg_quality, 77);
        assert!(!back.confirm_overwrite);
        assert_eq!(back.favorites, vec![favorite]);
    }

    #[test]
    fn toggle_favorite_pins_and_unpins() {
        let mut cfg = AppConfig::default();
        let dir = Path::new("/fotos");
        assert!(cfg.toggle_favorite(dir));
        assert!(cfg.is_favorite(dir));
        assert!(!cfg.toggle_favorite(dir));
        assert!(!cfg.is_favorite(dir));
    }

    #[test]
    fn legacy_config_missing_new_fields_uses_compatible_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let favorite = dir.path().join("legacy-favorite");
        std::fs::create_dir(&favorite).expect("mkdir favorite");
        let path = dir.path().join("config.json");
        let json = serde_json::json!({
            "favorites": [favorite],
            "jpeg_quality": 73,
            "confirm_overwrite": false
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&json).expect("json"))
            .expect("write legacy config");

        let loaded = AppConfig::load_from(&path).expect("load legacy config");

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.jpeg_quality, 73);
        assert!(!loaded.confirm_overwrite);
        assert!(loaded.show_filmstrip);
        assert!(loaded.open_last_on_startup);
        assert!(loaded.respect_gitignore);
        assert!(loaded.skip_hidden);
        assert_eq!(loaded.prefetch_max_mb, 64);
        assert_eq!(loaded.theme, "slate");
        assert_eq!(loaded.thumb_size, 88.0);
    }

    #[test]
    fn corrupt_file_falls_back_to_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{ invalido").expect("write");
        assert!(AppConfig::load_from(&path).is_none());
    }
}
