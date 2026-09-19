# Contribuindo com o photoshow

Valeu pelo interesse! O projeto é pequeno de propósito — leia o
[ROADMAP.md](./ROADMAP.md) antes para entender onde cada ideia se encaixa
(e o que é explicitamente não-objetivo).

## Desenvolvimento

Requer Rust 1.95+.

```bash
cargo test          # 28 testes (devem passar todos)
cargo fmt --all     # formatação obrigatória
cargo clippy --all-targets   # zero warnings
cargo run           # roda em modo debug
```

## Regras do código

- Tipos de domínio em vez de primitivos soltos (`PhotoPath`, não `String`)
- Erros recuperáveis via `Result` — sem `unwrap` em caminho de usuário
- Decode/save pesado sempre em thread, nunca na thread da UI
- Preview e save compartilham o mesmo código (`bake()`)
- Testes para lógica pura (orientação, crop, bake, scan, config)

## Pull requests

1. Abra uma issue curta primeiro se for recurso novo (ver ROADMAP)
2. PRs pequenos e focados, com testes quando houver lógica testável
3. `fmt` + `clippy` limpos, `cargo test` verde

## Apoie o projeto

Se o photoshow é útil para você, considere apoiar o desenvolvimento:

**[ko-fi.com/raillen](https://ko-fi.com/raillen)**

Toda contribuição — código, bug report bem descrito ou um café — mantém
o projeto leve e independente. Obrigado!
