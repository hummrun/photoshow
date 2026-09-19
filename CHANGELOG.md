# Changelog

Todos os lançamentos notáveis deste projeto. Formato baseado em
[Keep a Changelog](https://keepachangelog.com/pt-BR/1.0.0/).

## [Unreleased]

## [0.1.0-rc1] - 2026-09-19

Primeiro release candidate do visualizador.

### Adicionado
- Abertura de pastas (varredura assíncrona, sem travar a UI) e arquivos soltos
- Navegação por árvore de pastas com expansão lazy e pastas favoritas fixadas
- Viewer com zoom ancorado no cursor, pan, fullscreen e modo maximizado
- Galeria de miniaturas em grade fluida com tamanho configurável (48–192px)
- Edição não-destrutiva: rotate 90°, crop por arrasto (mover + gizmos,
  proporção opcional, Enter aplica), undo/redo, reset
- Salvar (com confirmação) e Salvar como… em thread, qualidade JPEG configurável
- Correção automática de orientação EXIF
- Renomear arquivos (F2) com validação de extensão
- Menu Arquivo único, menu de contexto (copiar caminho/imagem, abrir,
  mostrar na pasta), temas claro/escuro (egui-elegance), ícones Phosphor
- Painéis redimensionáveis via dock, layout restaurável
- Configuração persistente (favoritas, última pasta, preferências)
- Lista de fotos virtualizada e prefetch de vizinhos com teto de memória

### Notas
- Binário release Linux x86_64 (~22MB, `target/release/photoshow`)
- Requer Rust 1.95+ para compilar
