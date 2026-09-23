//! Folder favorites, lazy tree navigation and virtualized file list.

use super::*;

impl PhotoShowApp {
    /// Painel esquerdo: favoritas, árvore e lista de arquivos.
    /// Sem scroll externo: a árvore tem altura limitada e a lista de fotos
    /// ocupa todo o espaço restante.
    pub(super) fn show_browser(&mut self, ui: &mut egui::Ui) {
        Self::section_header(ui, "Favoritas");
        if self.cfg.favorites.is_empty() {
            ui.weak("Nenhuma pasta fixada.");
        }
        let weak = ui.visuals().weak_text_color();
        let mut unpin: Option<PathBuf> = None;
        let mut open_fav: Option<PathBuf> = None;
        for fav in &self.cfg.favorites {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(icons::P::FOLDER).size(13.0).color(weak));
                let name = fav
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| fav.display().to_string());
                if ui
                    .selectable_label(self.current_dir.as_ref() == Some(fav), name)
                    .on_hover_text(fav.display().to_string())
                    .clicked()
                {
                    open_fav = Some(fav.clone());
                }
                if ui
                    .small_button(icons::P::X)
                    .on_hover_text("Desafixar")
                    .clicked()
                {
                    unpin = Some(fav.clone());
                }
            });
        }
        if let Some(dir) = unpin {
            self.cfg.toggle_favorite(&dir);
            self.persist();
        }
        if let Some(dir) = open_fav {
            self.open_dir_path(ui.ctx(), dir);
            return;
        }
        ui.separator();

        if self.tree_root.is_some() {
            let root_name = self
                .tree_root
                .as_ref()
                .map(|r| r.name())
                .unwrap_or_default();
            let pinned = self
                .current_dir
                .as_ref()
                .map(|d| self.cfg.is_favorite(d))
                .unwrap_or(false);
            let mut pin_toggle = false;
            Self::section_header(ui, "Pasta atual");
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(icons::P::FOLDER_OPEN)
                        .size(13.0)
                        .color(weak),
                );
                ui.strong(root_name);
                let star = egui::RichText::new(icons::P::STAR).color(if pinned {
                    egui::Color32::YELLOW
                } else {
                    weak
                });
                if ui
                    .small_button(star)
                    .on_hover_text("Fixar/desafixar pasta atual")
                    .clicked()
                {
                    pin_toggle = true;
                }
            });
            if pin_toggle && let Some(dir) = self.current_dir.clone() {
                let fixed = self.cfg.toggle_favorite(&dir);
                self.persist();
                self.status = if fixed {
                    format!("Pasta fixada: {}", dir.display())
                } else {
                    String::from("Pasta desafixada.")
                };
            }
            Self::section_header(ui, "Subpastas");
            let tree_h = (ui.available_height() * 0.34).clamp(90.0, 280.0);
            let action = egui::ScrollArea::vertical()
                .max_height(tree_h)
                .show(ui, |ui| {
                    Self::show_node(
                        ui,
                        self.tree_root.as_mut().expect("root"),
                        self.current_dir.as_ref(),
                        0,
                        self.cfg.show_hidden_folders,
                    )
                })
                .inner;
            match action {
                Some(TreeAction::Toggle(i)) => {
                    if let Some(root) = self.tree_root.as_mut() {
                        Self::toggle_node(root, i);
                    }
                }
                Some(TreeAction::Open(dir)) => {
                    self.current_dir = Some(dir.clone());
                    self.load_folder_contents(ui.ctx(), &dir);
                }
                None => {}
            }
        } else {
            ui.weak("Nenhuma pasta aberta.");
        }
        ui.separator();

        ui.horizontal(|ui| {
            Self::section_header(ui, &format!("Fotos ({})", self.visible.len()));
            if self.scanning.is_some() {
                ui.spinner();
            }
        });
        // Lista virtualizada preenchendo o restante: 50k fotos custam ~40 linhas.
        let clicked = egui::ScrollArea::vertical()
            .show_rows(ui, 24.0, self.visible.len(), |ui, range| {
                let mut clicked: Option<(usize, PhotoPath)> = None;
                for i in range {
                    let photo = &self.visible[i];
                    if ui
                        .selectable_label(Some(i) == self.sel, photo.display_name())
                        .clicked()
                    {
                        clicked = Some((i, photo.clone()));
                    }
                }
                clicked
            })
            .inner;
        if let Some((i, p)) = clicked {
            self.select_photo(ui.ctx(), i, p);
        }
    }

    /// Renderiza um nó da árvore; retorna a ação do usuário (índice global).
    fn show_node(
        ui: &mut egui::Ui,
        node: &mut DirNode,
        current: Option<&PathBuf>,
        depth: usize,
        show_hidden: bool,
    ) -> Option<TreeAction> {
        Self::show_node_inner(ui, node, current, depth, show_hidden, &mut 0)
    }

    fn show_node_inner(
        ui: &mut egui::Ui,
        node: &mut DirNode,
        current: Option<&PathBuf>,
        depth: usize,
        show_hidden: bool,
        counter: &mut usize,
    ) -> Option<TreeAction> {
        let my_idx = *counter;
        *counter += 1;
        let mut action = None;
        let subdirs = || {
            fs_browser::list_subdirs(&node.path)
                .into_iter()
                .filter(|p| show_hidden || !fs_browser::is_hidden(p))
        };
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 12.0);
            let kids = subdirs().next().is_some() || node.children.is_some();
            if kids {
                let glyph = if node.expanded {
                    icons::P::CARET_DOWN
                } else {
                    icons::P::CARET_RIGHT
                };
                let caret = egui::RichText::new(glyph)
                    .size(12.0)
                    .color(ui.visuals().weak_text_color());
                if ui
                    .add_sized([18.0, 20.0], egui::Button::new(caret).frame(false))
                    .clicked()
                {
                    action = Some(TreeAction::Toggle(my_idx));
                }
            } else {
                ui.add_space(18.0);
            }
            ui.label(
                egui::RichText::new(icons::P::FOLDER)
                    .size(13.0)
                    .color(ui.visuals().weak_text_color()),
            );
            if ui
                .selectable_label(current == Some(&node.path), node.name())
                .clicked()
            {
                action = Some(TreeAction::Open(node.path.clone()));
            }
        });
        if node.expanded {
            if node.children.is_none() {
                node.children = Some(subdirs().map(DirNode::new).collect());
            }
            if let Some(kids) = node.children.as_mut() {
                for kid in kids.iter_mut() {
                    if let Some(a) =
                        Self::show_node_inner(ui, kid, current, depth + 1, show_hidden, counter)
                    {
                        action = Some(a);
                        break;
                    }
                }
            }
        }
        action
    }

    /// Alterna expandido do n-ésimo nó (pré-ordem).
    fn toggle_node(root: &mut DirNode, target: usize) {
        let mut counter = 0;
        Self::toggle_inner(root, target, &mut counter);
    }

    fn toggle_inner(node: &mut DirNode, target: usize, counter: &mut usize) -> bool {
        if *counter == target {
            node.expanded = !node.expanded;
            return true;
        }
        *counter += 1;
        if node.expanded
            && let Some(kids) = node.children.as_mut()
        {
            for kid in kids.iter_mut() {
                if Self::toggle_inner(kid, target, counter) {
                    return true;
                }
            }
        }
        false
    }
}
