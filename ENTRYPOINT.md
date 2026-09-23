# PhotoShow — Prumo Entrypoint

1. Leia `prumo.json`, `PROJECT_STATE.md` e `docs/PRUMO.md`.
2. Resolva a tarefa contra a arquitetura e o roadmap canônicos antes de editar código.
3. Use o menor contexto suficiente; não carregue o repositório inteiro por padrão.
4. Trate `.ai/skills/` como projeções/imports legados até serem regenerados pelo Prumo atual.
5. Não declare uma mudança concluída sem os gates relevantes: format, Clippy, testes e build; alterações de performance exigem evidência.
6. Mudanças em UI devem atualizar/verificar `docs/ui-ux/interface-map.json` e a matriz de estados quando afetadas.
7. Mudanças de escrita em disco, cache, decode, release ou installer são de risco elevado e exigem testes de falha/rollback.
8. Não faça rewrite big-bang do frontend. O toolkit canônico continua egui até uma decisão explícita sustentada por evidência.

## Ordem de autoridade

1. Testes e comportamento implementado verificável.
2. `prumo.json` e contratos canônicos em `docs/`.
3. ADRs/roadmap aprovados.
4. README e documentação de uso.
5. Projeções e skills importadas.
6. Hipóteses de agents.

Silêncio documental significa desconhecido, não permissão para inventar arquitetura.
