# PhotoShow — Roadmap até 2.0

> **Visão:** visualizador nativo ultrarrápido com organização prática e edição leve.
> **Anti-visão:** não virar Lightroom/GIMP/DAM. Cada recurso precisa justificar seu
> custo de memória, CPU, dependências e complexidade de interface.

Estado de produto: `0.1.0-rc1`.

Estado de engenharia: **P01 — Reliability, Performance & Architecture Hardening**.
Nenhuma feature grande deve furar os gates desta fase.

---

## P01 — hardening obrigatório antes de novas features

### Confiabilidade e integridade

- [x] CI contínuo em Linux e Windows: fmt, Clippy `-D warnings`, testes e release build
- [x] MSRV 1.95 verificado em CI
- [x] persistência da configuração por replace seguro
- [x] save da imagem por arquivo temporário no mesmo filesystem + promoção/rollback
- [x] installer com validação estrutural do pacote
- [x] checksums SHA-256 nas releases
- [x] smoke test do tarball/installer antes de publicação
- [x] metadata de save deixou de ser silenciosa: EXIF/ICC são preservados quando seguro/suportado e omissões são informadas
- [ ] teste de falha de encode imediatamente antes da promoção final
- [ ] smoke real do workflow de release em tag de pré-release

### Performance e memória

- [x] full-resolution deixou de permanecer residente no viewer
- [x] full-resolution é decodificada sob demanda para save/copy
- [x] loader principal `latest-wins` em vez de uma thread nova por seleção
- [x] prefetch usa orçamento por bytes decodificados, não pelo tamanho comprimido
- [x] prefetch rejeita resultados de gerações/pastas obsoletas
- [x] buffer RGBA de upload é temporário; não existe segunda cópia CPU permanente
- [x] filmstrip virtualizado pelas linhas realmente visíveis
- [x] scheduler de thumbnails segue o viewport
- [x] fila de thumbnails é limitada e rejeita trabalho de pasta obsoleta antes do decode
- [x] scan reporta erros de IO/permissão em vez de descartá-los silenciosamente
- [x] sort do scan usa chave normalizada cacheada
- [x] registrar baseline sintético reproduzível de scan 1k/10k/50k
- [ ] registrar baseline de startup/idle RSS/viewer/gallery em hardware representativo conforme `docs/quality/performance-baseline.md`
- [ ] comparar WGPU × Glow em hardware moderno e low-end antes de decidir qualquer mudança de renderer

### Arquitetura / Prumo

- [x] `project-profile.json` migrado
- [x] `prumo.json`, `ENTRYPOINT.md`, `PROJECT_STATE.md` e `docs/PRUMO.md`
- [x] workforce/manifests resolvidos pelo catálogo Prumo atual
- [x] arquitetura canônica e estratégia de testes
- [x] interface map + state matrix
- [x] `platform.rs` separa integração com SO
- [x] tema separado em `ui/theme.rs`
- [x] browser/viewer/filmstrip extraídos do monólito `app.rs`
- [ ] separar decode/media neutros de `image_store` (adapter egui)
- [ ] reduzir o estado central restante sem criar abstrações artificiais
- [ ] reconciliar/regenerar integralmente skills importadas antigas quando o Prumo expuser o fluxo adequado
- [ ] baseline de acessibilidade por teclado/foco nos fluxos críticos

### Gate de saída P01

Para sair desta fase, todos devem ser verdadeiros:

1. CI Rust verde em Linux e Windows.
2. `prumo validate` e `prumo doctor` verdes.
3. Interface map declarado válido; derivação de símbolos Rust fica explicitamente desativada enquanto o Prumo só tiver `GoSymbolDeriver`.
4. overwrite não destrói o original em falha.
5. pasta grande não materializa O(n) widgets por frame.
6. filas de decode são limitadas/descartam trabalho obsoleto.
7. existe pelo menos um relatório de performance reproduzível.
8. instalação a partir do artefato da release passa em smoke test.

---

## 0.1.0 — estabilização final da rc1

Depois do P01, fechar somente o que melhora a experiência existente:

- [ ] persistir layout do dock
- [ ] galeria rolar até o thumbnail selecionado quando a navegação veio de fora dela
- [ ] migração/versionamento explícito do arquivo de configuração
- [ ] janela compacta de atalhos (`F1` ou `?`)
- [ ] QA X11 + Wayland + Windows
- [ ] verificar `cargo install --path .` do zero
- [ ] liberar `v0.1.0`

**Critério de saída:** instalar → abrir uma pasta grande → navegar → rotate/crop →
salvar/salvar como → reiniciar, sem freeze, corrupção ou estado incoerente.

---

## 0.2 — encontrar e entender fotos

A ordem é deliberada: primeiro recursos **read-only**, baratos e fáceis de
validar; persistência por foto só entra quando a navegação está estável.

### 0.2.1 — ordenação

- [ ] nome A–Z / Z–A
- [ ] data de modificação
- [ ] tamanho
- [ ] preservar seleção ao trocar sort
- [ ] ordenar índices/metadados, não duplicar pixel data

### 0.2.2 — busca

- [ ] filtro incremental por nome
- [ ] debounce apenas se as medições mostrarem necessidade
- [ ] busca combinável com filtro de formato
- [ ] estado vazio explícito: pasta vazia ≠ nenhum resultado da busca

### 0.2.3 — painel de metadata

- [ ] dimensões
- [ ] formato/tamanho do arquivo
- [ ] data
- [ ] câmera/lente quando existente
- [ ] exposição, abertura, ISO e focal quando existente
- [ ] separar “ausente” de “erro ao ler”

### 0.2.4 — rating/favorito por foto

- [ ] 1–5 estrelas + favorito
- [ ] definir sidecar estável/versionado antes da implementação
- [ ] escrita atômica do sidecar
- [ ] nunca alterar o arquivo original apenas para registrar rating
- [ ] estratégia clara para rename/move do arquivo

---

## 0.3 — apresentação

- [ ] slideshow com temporizador configurável
- [ ] transições limitadas a corte/fade
- [ ] pré-carregamento usa o mesmo orçamento do viewer
- [ ] modo apresentação com UI mínima e saída por `Esc`/`F11`

---

## 0.4 — edição básica não-destrutiva

Estender o `EditorState`; preview e bake final precisam compartilhar semântica.

- [ ] exposição
- [ ] contraste
- [ ] saturação
- [ ] temperatura/tint
- [ ] histograma RGB
- [ ] clipping de highlights/shadows
- [ ] comparação original × editado
- [ ] processamento de preview fora do frame da UI se a medição indicar jank

**Invariante:** nenhum ajuste vira ferramenta local, máscara ou sistema de layers.

---

## 0.5 — lote

- [ ] fila limitada de operações
- [ ] rotate
- [ ] conversão
- [ ] resize
- [ ] rename por padrão
- [ ] progresso + cancelamento cooperativo
- [ ] relatório por arquivo
- [ ] limite explícito de concorrência/memória

---

## 1.0 — polimento e distribuição

- [ ] Flatpak e/ou AppImage conforme demanda real
- [ ] testes de integração dos fluxos abrir → editar → salvar
- [ ] documentação final de atalhos, formatos e performance
- [ ] auditoria de pasta 50k + TIFF 200 MB+
- [ ] budgets de regressão promovidos a partir dos baselines medidos
- [ ] congelar escopo 1.x

**Critério de saída:** viewer/organizador/editor leve confiável para
JPEG/PNG/WebP/TIFF, instalável sem ambiente de desenvolvimento.

---

## 2.0 — RAW opcional, condicionado a nova pesquisa

RAW continua sendo hipótese de produto, não compromisso da 1.x.

### Princípios

- preview embutido primeiro
- full RAW somente para a foto ativa
- pipeline cancelável e com concorrência limitada
- feature opcional; build padrão não paga o custo
- sem catálogo, cloud, IA, máscaras ou banco pesado de lentes

Antes de qualquer implementação deve ser reavaliado o ecossistema Rust de RAW
existente na época; o roadmap não fixa hoje `rawloader` ou outro decoder como
decisão arquitetural eterna.

---

## Regras para aceitar uma nova feature

Uma feature só entra quando responde claramente:

1. Qual problema de viewer/organização/edição leve ela resolve?
2. Qual o custo de memória/CPU/startup/binário?
3. Ela exige estado persistente novo?
4. Qual é o comportamento de erro/cancelamento?
5. Quais contratos/UI states mudam?
6. Como será testada?
7. Qual evidência prova que não degradou pasta grande e hardware modesto?

Se essas respostas exigirem transformar o PhotoShow em DAM/editor pesado, a
feature está fora da visão do produto.
