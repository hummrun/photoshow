//! Viewer interactions, zoom/pan, crop overlay and image context actions.

use super::*;

impl PhotoShowApp {
    /// Conteúdo da aba Visualizador (placeholder, erro amigável ou viewer).
    pub(super) fn viewer_tab_content(&mut self, ui: &mut egui::Ui) {
        // Botão flutuante para sair do modo maximizado.
        if self.maximized {
            egui::Window::new("restore_panels")
                .anchor(egui::Align2::CENTER_TOP, egui::Vec2::new(0.0, 8.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .show(ui.ctx(), |ui| {
                    if ui
                        .button(labeled(icons::P::SQUARES_FOUR, "Restaurar painéis"))
                        .on_hover_text("F9")
                        .clicked()
                    {
                        self.toggle_maximize();
                    }
                });
        }
        match self.store.state() {
            LoadState::Empty => {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        "Nenhuma foto — abra uma pasta ou fixe uma favorita. (F11 = fullscreen)",
                    );
                });
            }
            LoadState::Loading => {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                });
            }
            LoadState::Failed(e) => {
                // Card neutro (nada vermelho gritando): detalhe vai para a statusbar.
                self.status = format!("Falha ao carregar: {e}");
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new(icons::P::IMAGE_BROKEN)
                                .size(40.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.add_space(8.0);
                        ui.strong("Não foi possível abrir esta imagem");
                        ui.weak("arquivo ilegível, incompleto ou corrompido");
                        ui.add_space(8.0);
                        ui.weak("← → para continuar navegando");
                    });
                });
            }
            LoadState::Loaded {
                texture,
                display_px,
                ..
            } => {
                self.show_viewer(ui, &texture, display_px);
            }
        }
    }

    /// Viewer central com zoom/pan, gesto de crop e menu de contexto.
    #[allow(clippy::too_many_lines)]
    fn show_viewer(
        &mut self,
        ui: &mut egui::Ui,
        texture: &egui::TextureHandle,
        display_px: egui::Vec2,
    ) {
        let avail = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(avail, egui::Sense::drag());

        // Zoom: scroll do mouse + pinch do trackpad, ancorado no cursor.
        let mut factor = ui.input(|i| i.zoom_delta());
        let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_y != 0.0 {
            factor *= 1.0 + scroll_y * 0.0015;
        }
        if factor != 1.0 && response.hovered() {
            let center = rect.center();
            let cursor = response.hover_pos().unwrap_or(center);
            let anchor = cursor - center;
            self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
            self.offset = anchor + (self.offset - anchor) * factor;
        }

        let fit = (rect.width() / display_px.x).min(rect.height() / display_px.y);
        let size = display_px * fit * self.zoom;
        let draw = egui::Rect::from_center_size(rect.center() + self.offset, size);
        self.last_draw = Some((draw, (display_px.x as u32, display_px.y as u32)));

        if self.crop_mode {
            self.crop_gesture(&response, &draw);
        } else {
            if response.dragged() {
                self.offset += response.drag_delta();
            }
            if response.double_clicked() {
                self.zoom = 1.0;
                self.offset = egui::Vec2::ZERO;
            }
        }

        let painter = ui.painter_at(rect);
        painter.image(
            texture.id(),
            draw,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        // Overlay do crop: escurece fora + borda + gizmos.
        if self.crop_mode
            && let Some(cr) = self.crop_rect
        {
            let cr = cr.intersect(draw);
            let dim = egui::Color32::from_black_alpha(140);
            painter.rect_filled(
                egui::Rect::from_two_pos(rect.min, egui::Pos2::new(cr.min.x, rect.max.y)),
                0.0,
                dim,
            );
            painter.rect_filled(
                egui::Rect::from_two_pos(egui::Pos2::new(cr.max.x, rect.min.y), rect.max),
                0.0,
                dim,
            );
            painter.rect_filled(
                egui::Rect::from_two_pos(
                    egui::Pos2::new(cr.min.x, rect.min.y),
                    egui::Pos2::new(cr.max.x, cr.min.y),
                ),
                0.0,
                dim,
            );
            let top = cr.max.y.min(rect.max.y);
            painter.rect_filled(
                egui::Rect::from_two_pos(
                    egui::Pos2::new(cr.min.x, top),
                    egui::Pos2::new(cr.max.x, rect.max.y),
                ),
                0.0,
                dim,
            );
            painter.rect_stroke(
                cr,
                0.0,
                egui::Stroke::new(2.0, egui::Color32::WHITE),
                egui::StrokeKind::Outside,
            );
            for h in Handle::all() {
                let p = h.point(&cr);
                let sq = egui::Rect::from_center_size(p, egui::Vec2::splat(9.0));
                painter.rect_filled(sq, 2.0, egui::Color32::WHITE);
                painter.rect_stroke(
                    sq,
                    2.0,
                    egui::Stroke::new(1.5, egui::Color32::BLACK),
                    egui::StrokeKind::Outside,
                );
            }
        }

        // Menu de contexto (botão direito) sobre a imagem.
        let mut action: Option<ImgAction> = None;
        response.context_menu(|ui| {
            if ui
                .button(labeled(icons::P::CLIPBOARD, "Copiar caminho"))
                .clicked()
            {
                action = Some(ImgAction::CopyPath);
                ui.close();
            }
            if ui
                .button(labeled(icons::P::FILE_IMAGE, "Copiar imagem"))
                .clicked()
            {
                action = Some(ImgAction::CopyImage);
                ui.close();
            }
            ui.separator();
            if ui
                .button(labeled(
                    icons::P::ARROW_SQUARE_OUT,
                    "Abrir com aplicativo padrão",
                ))
                .clicked()
            {
                action = Some(ImgAction::OpenDefault);
                ui.close();
            }
            if ui
                .button(labeled(icons::P::FOLDER_OPEN, "Mostrar na pasta"))
                .clicked()
            {
                action = Some(ImgAction::Reveal);
                ui.close();
            }
            ui.separator();
            if ui
                .button(labeled(icons::P::PENCIL_LINE, "Renomear…"))
                .clicked()
            {
                action = Some(ImgAction::Rename);
                ui.close();
            }
        });
        if let Some(a) = action {
            self.run_img_action(ui.ctx(), a);
        }
    }

    /// Máquina de estados do gesto de crop (novo / mover / gizmo).
    fn crop_gesture(&mut self, response: &egui::Response, draw: &egui::Rect) {
        if response.drag_started()
            && let Some(p) = response.hover_pos()
        {
            if let Some(r) = self.crop_rect {
                if let Some((_, anchor)) = hit_handle(&r, p) {
                    self.crop_drag = Some(CropDrag::Resize(anchor));
                } else if r.contains(p) {
                    self.crop_drag = Some(CropDrag::Move(p - r.min));
                } else {
                    self.crop_drag = Some(CropDrag::New(p));
                    self.crop_rect = None;
                }
            } else {
                self.crop_drag = Some(CropDrag::New(p));
            }
        }
        if response.dragged()
            && let Some(hover) = response.hover_pos()
        {
            let ratio = self.crop_ratio();
            match self.crop_drag {
                Some(CropDrag::New(a)) => {
                    self.crop_rect = Some(enforce_aspect(a, hover, ratio).intersect(*draw));
                }
                Some(CropDrag::Move(off)) => {
                    if let Some(r) = self.crop_rect {
                        let size = r.size();
                        let min = (hover - off).clamp(draw.min, draw.max - size);
                        self.crop_rect = Some(egui::Rect::from_min_size(min, size));
                    }
                }
                Some(CropDrag::Resize(anchor)) => {
                    self.crop_rect = Some(enforce_aspect(anchor, hover, ratio).intersect(*draw));
                }
                None => {}
            }
        }
        if response.drag_stopped() {
            self.crop_drag = None;
        }
    }

    /// Executa a ação do menu de contexto.
    fn run_img_action(&mut self, ctx: &egui::Context, action: ImgAction) {
        let Some(cur) = self.current.clone() else {
            return;
        };
        match action {
            ImgAction::CopyPath => {
                let s = cur.path().display().to_string();
                match platform::copy_text_to_clipboard(&s) {
                    Ok(()) => self.status = String::from("Caminho copiado."),
                    Err(e) => self.status = e,
                }
            }
            ImgAction::CopyImage => {
                if self.copying {
                    self.status = String::from("Cópia de imagem já em andamento…");
                    return;
                }
                let path = cur.path().to_path_buf();
                let tx = self.copy_tx.clone();
                let repaint = ctx.clone();
                self.copying = true;
                self.status = String::from("Preparando imagem para o clipboard…");
                std::thread::spawn(move || {
                    let note = match decode_full_photo(&path) {
                        Ok(image) => match platform::copy_image_to_clipboard(&image) {
                            Ok(()) => String::from("Imagem copiada."),
                            Err(error) => format!("Falha ao copiar imagem: {error}"),
                        },
                        Err(error) => format!("Falha ao decodificar imagem: {error}"),
                    };
                    let _ = tx.send(CopyMsg { note });
                    repaint.request_repaint();
                });
            }
            ImgAction::OpenDefault => {
                if let Err(error) = platform::open_default(cur.path()) {
                    self.status = error;
                }
            }
            ImgAction::Reveal => {
                if let Err(e) = platform::reveal_in_folder(cur.path()) {
                    self.status = e;
                }
            }
            ImgAction::Rename => self.open_rename(),
        }
    }
}
