# photoshow — Roadmap até 2.0

> **Visão:** visualizador ultrarrápido com organização prática e edição leve.
> **Anti-visão:** nunca virar GIMP/Lightroom — cada milestone entrega fluxos
> completos, nunca meio-recurso, nunca bloat.

Estado atual: `0.1.0-rc1` (navegação, filmstrip/galeria, rotate/crop
não-destrutivo, EXIF, favoritas, temas, dock redimensionável).

---

## 0.1.0 — Estabilização da rc1 (definitiva)

Foco: transformar a rc1 em software confiável e distribuível. Nada de
recurso novo grande.

### Persistência e estado
- [ ] Persistir layout do dock (posição/tamanho dos painéis) no config
      (`egui_dock` tem feature `serde`; salvar em `~/.config/photoshow/`)
- [ ] Galeria acompanha a seleção: rolar até o thumb ativo ao navegar
      por setas/clique na lista
- [ ] Migração tolerante de config antiga (campos novos com default)

### Fidelidade de arquivo
- [ ] Preservar metadata EXIF no save (hoje o `bake()` descarta tudo;
      copiar tags essenciais ou reanexar via `kamadak-exif`/`little-exif`)
- [ ] Decidir `Delete` → lixeira: entra na 0.1.0 (via `trash` crate) ou
      fica para 0.2? (proposta: entra — é esperado em qualquer viewer)

### Empacotamento e repo público
- [ ] `LICENSE-MIT` + `LICENSE-APACHE` (padrão do ecossistema egui)
- [ ] `README.md` com screenshot, recursos, build e atalhos
- [ ] `CHANGELOG.md` (formato Keep a Changelog)
- [ ] `.desktop` + ícone para Linux
- [ ] Release 0.1.0 no GitHub com binário anexado
- [ ] `cargo install --path .` verificado do zero (deps do sistema documentadas)

### Validação
- [ ] QA manual em pastas gigantes reais (cenário do batch de performance)
- [ ] Smoke em X11 além de Wayland
- [ ] Auditoria de atalhos: listar todos numa janela de ajuda (`F1` ou `?`)

**Critério de saída:** instalar do zero → abrir pasta de 50k arquivos →
navegar, editar, salvar — sem freeze, sem erro vermelho, sem perda de metadata.

---

## 0.2 — Organização

- [ ] Nota/favorito por foto (1–5 estrelas), persistido em sidecar JSON
      ao lado do arquivo (nunca banco escondido, nunca toca no original)
- [ ] Ordenação: nome, data, tamanho
- [ ] Busca por nome (filtro incremental na barra)
- [ ] Painel de metadados EXIF (dimensões, câmera, exposição, ISO, data)

## 0.3 — Apresentação

- [ ] Slideshow com temporizador configurável e transições simples
      (fade/corte; nada de motor de efeitos)
- [ ] Modo apresentação: UI mínima, `Esc`/`F11` sai

## 0.4 — Edição básica real (ainda não-destrutiva)

Tudo global, tudo na `EditorState` existente (stack + undo/redo + preview):

- [ ] Exposição (EV), contraste, saturação, temperatura/tint (para JPEG:
      aproximação via balanço de canais)
- [ ] Histograma RGB + overlay de clipping (estourados/sombras)
- [ ] Comparador lado a lado / split (original × editado, arrastável)
- [ ] `bake()` estendido cobre os novos ops (preview e save usam o mesmo código)

## 0.5 — Lote

- [ ] Fila de operações em lote: rotacionar, converter formato,
      redimensionar, renomear com padrão (`viagem_###.jpg`)
- [ ] Progresso com cancelamento, em thread (nunca trava a UI)
- [ ] Relatório final (ok/falhas por arquivo)

## 1.0 — Polimento e distribuição

- [ ] Flatpak e/ou AppImage
- [ ] Testes de integração nos fluxos críticos (abrir→editar→salvar)
- [ ] Docs de usuário (atalhos, formatos, FAQ de performance)
- [ ] Auditoria de performance final (pastas gigantes + TIFFs de 200MB+)
- [ ] Congelar escopo: tudo que não coube vira proposta para 2.x

**Critério de saída da 1.0:** organizador + editor leve completo para
JPEG/PNG/WebP/TIFF, instalável em 1 comando, sem dependência de terminal.

---

## 2.0 (conjectura) — Editor RAW enxuto, sem virar Lightroom

> Princípio: RAW é um **modo a mais do viewer**, não um produto novo.
> O binário padrão continua sem ele (feature flag), e o ajuste máximo
> é global — sem máscaras, sem IA, sem banco de lentes.

### Por que dá para ser leve

1. **Preview embutido primeiro:** todo RAW traz um JPEG embutido. A
   navegação/galeria/thumbs usam ele — custo zero, velocidade igual à de JPEG.
2. **Decode total só sob demanda:** só a foto selecionada (e só ao entrar
   em modo RAW) passa pelo pipeline completo, em thread, com cache de 1.
3. **Feature flag `raw`:** `rawloader` (Rust puro, sem LibRaw/C) entra só com
   `--features raw`. O build padrão continua magro.
4. **Sem DB de lentes/câmeras pesado:** sem `lensfun`, sem perfis DCP —
   matriz de cor vem do próprio arquivo (`rawloader` expõe) + fallback sRGB.

### Pipeline proposto (`src/raw/`, ~4 ajustes, todos puros e testáveis)

```
CFA ──► demosaic bilinear c/ equilíbrio de verde ──► linear RGB
  ──► balanço de branco (multiplicadores do metadata + picker cinza)
  ──► exposição (EV) + recuperação de highlights (clip guiado)
  ──► nível de preto ──► tone-map fílmico simples ──► sRGB ──► textura
```

- Demosaic próprio (~200 linhas, testável em CFA sintético) em vez de
  puxar crate pesado; qualidade "boa", não "estado da arte" — documentado.
- Cada etapa é função pura `&[f32] -> Vec<f32>`: teste unitário barato,
  preview e bake compartilham o código (igual ao `bake()` atual).
- Reuso total da infra existente: threads de decode, `EditorState`
  estendido (`ev`, `wb_temp`, `wb_tint`, `highlights`), histograma e
  clipping da 0.4, save em thread.

### Conjunto fechado de ajustes (não cresce)

Exposição, temperatura/tint, highlights, sombras, contraste, saturação,
curva de tons simples, crop/rotate (os atuais). Ponto.

### Formatos (limitados ao que `rawloader` cobre bem)

NEF, CR2, ARW, RAF, RW2, DNG. **Fora:** CR3 comprimido total (limitação
conhecida do `rawloader` — documentar, não prometer).

### Orçamento de performance (regras duras)

- Abrir pasta com RAWs: tão rápido quanto JPEG (só previews embutidos)
- Thumb RAW: usa preview embutido, nunca decode total
- Decode total: só foto atual, só em modo RAW, cancelável ao navegar
- Binário padrão sem `raw`: zero bytes a mais

### Explicitamente FORA (para não virar bloat)

Correção de lente por banco de dados, denoise com IA, ajustes locais/
máscaras, catálogo com banco de dados, importação com presets, edição
de vídeo, integração com nuvem, plugins.

---

## Como acompanhar

- `0.1.0`: issues com checklist acima, marco `v0.1.0` no GitHub
- `0.2`–`1.0`: uma issue de design curta por milestone antes de codar
- `2.0`: tudo aqui é conjectura — revalidar `rawloader` e escopo antes
  de qualquer linha de código
