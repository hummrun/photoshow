//! Viewport-virtualized thumbnail gallery.

use super::*;

impl PhotoShowApp {
    /// Galeria de miniaturas virtualizada por linhas do viewport.
    ///
    /// Apenas as linhas visíveis viram widgets. O intervalo de itens resultante
    /// alimenta o scheduler de thumbnails no fim do frame.
    pub(super) fn show_filmstrip_content(&mut self, ui: &mut egui::Ui) {
        if self.visible.is_empty() {
            ui.weak("Nenhuma foto.");
            return;
        }
        if !self.cfg.show_filmstrip {
            ui.weak("Galeria desativada — ative em Config.");
            return;
        }

        ui.horizontal(|ui| {
            ui.weak("Tamanho:");
            if ui
                .small_button(icons::P::MINUS)
                .on_hover_text("Diminuir miniaturas")
                .clicked()
            {
                self.set_thumb_size(self.cfg.thumb_size - 16.0);
            }
            let mut size = self.cfg.thumb_size;
            if ui
                .add(egui::Slider::new(&mut size, 48.0..=192.0).show_value(false))
                .changed()
            {
                self.set_thumb_size(size);
            }
            if ui
                .small_button(icons::P::PLUS)
                .on_hover_text("Aumentar miniaturas")
                .clicked()
            {
                self.set_thumb_size(self.cfg.thumb_size + 16.0);
            }
            ui.weak(format!("{:.0}px", self.cfg.thumb_size));
        });
        ui.separator();

        let cell = self.cfg.thumb_size;
        let gap = 8.0;
        let columns = ((ui.available_width() + gap) / (cell + gap))
            .floor()
            .max(1.0) as usize;
        let total_rows = self.visible.len().div_ceil(columns);
        let selected = self.sel;
        let mut clicked: Option<(usize, PhotoPath, bool)> = None;
        let mut viewport = None;

        egui::ScrollArea::vertical().show_rows(ui, cell + gap, total_rows, |ui, row_range| {
            let first = row_range.start.saturating_mul(columns);
            let end = row_range
                .end
                .saturating_mul(columns)
                .min(self.visible.len());
            viewport = Some((first, end));

            for row in row_range {
                let start = row.saturating_mul(columns);
                let end = start.saturating_add(columns).min(self.visible.len());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for index in start..end {
                        let photo = &self.visible[index];
                        let is_selected = Some(index) == selected;
                        let response = match self.thumbs.get(photo.path()) {
                            Some(texture) => {
                                let image =
                                    egui::Image::from_texture(egui::load::SizedTexture::new(
                                        texture.id(),
                                        egui::Vec2::splat(cell - 4.0),
                                    ));
                                ui.add_sized(
                                    [cell - 4.0, cell - 4.0],
                                    egui::Button::new(image).frame(false),
                                )
                            }
                            None => {
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::Vec2::splat(cell - 4.0),
                                    egui::Sense::click(),
                                );
                                ui.painter()
                                    .rect_filled(rect, 6.0, egui::Color32::from_gray(42));
                                response
                            }
                        };

                        if is_selected {
                            ui.painter().rect_stroke(
                                response.rect.expand(2.0),
                                8.0,
                                egui::Stroke::new(2.5, theme::ACCENT),
                                egui::StrokeKind::Outside,
                            );
                        }
                        if response.clicked() {
                            clicked = Some((index, photo.clone(), response.double_clicked()));
                        }
                    }
                });
            }
        });

        self.thumb_viewport = viewport;

        if let Some((index, photo, double)) = clicked {
            self.select_photo(ui.ctx(), index, photo);
            if double && !self.maximized {
                self.toggle_maximize();
            }
        }
    }}
