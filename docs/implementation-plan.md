# AlgoritmosLab — Plano de Implementação

> **Base:** [design.md](./design.md)  
> **Baseline e ambiente verificado em 2026-11-11:** Fases 0–5 concluídas; ambiente Ubuntu verificado com `node v24.22.1`, `npm 11.19.1`, `rustc 1.99.1`, `cargo 1.99.1`, `clippy` e `webkit2gtk-4.1` (`2.52.6`) instalados. As seções abaixo preservam a estrutura original das fases como referência histórica de planejamento.

---

## Estrutura Alvo do Repositório

```text
algoritmoslab/
├── crates/
│   ├── algoritmoslab-core/    # Biblioteca pura em Rust (lexer, parser, AST, VM, depurador, builtins)
│   └── algoritmoslab-cli/     # Runner CLI mínimo para testes Golden (.alg) e validação rápida
├── src-tauri/                 # Host desktop Tauri v2 (io.github.imarcosvinicius.AlgoritmosLab)
├── src/                       # Frontend React + TypeScript + CodeMirror 6
├── tests/
│   └── golden/                # Suíte .alg (UTF-8 e Windows-1252) + entradas/saídas esperadas
├── docs/
│   ├── design.md
│   └── implementation-plan.md
├── LICENSE                    # GPL-3.0-only
└── README.md
```

---

## Fase 0 — Preparação do Ambiente e Skeleton

**Objetivo:** Deixar o ambiente Ubuntu pronto e o esqueleto Tauri v2 + workspace Rust compilando.

1. **Instalação do toolchain (com autorização explícita antes de executar):**
   - Instalar Rust estável (`rustup` / `rustc` / `cargo`).
   - Instalar pacotes de sistema do Tauri v2 no Ubuntu (`libwebkit2gtk-4.1-dev`, `build-essential`, `curl`, `wget`, `file`, `libxdo-dev`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`).
2. **Inicialização do projeto:**
   - Criar `Cargo.toml` (workspace com `crates/algoritmoslab-core` e `crates/algoritmoslab-cli`).
   - Inicializar frontend React + TypeScript + Vite e `src-tauri` com identificador `io.github.imarcosvinicius.AlgoritmosLab` e título `AlgoritmosLab`.
   - Adicionar `LICENSE` (GPL-3.0) e configuração inicial de linter/formatação (`cargo clippy`, `eslint`/TypeScript estrito).

**Critério de pronto:** `cargo test` executa sem erros e `npm run tauri dev` abre a janela básica do **AlgoritmosLab** no Ubuntu.

---

## Fase 1 — Núcleo `algoritmoslab-core` v0.1 + Suíte Golden Base

**Objetivo:** Construir o interpretador e depurador `.alg` de forma isolada, sem depender da UI.

1. **Lexer e Parser (`algoritmoslab-core`):**
   - Suporte *case-insensitive* e tolerância a acentos nas palavras-chave.
   - Bloco `algoritmo "..."`, seção `var` (`inteiro`, `real`, `caractere`, `logico`, `vetor[min..max]`, `matriz[r1..r2, c1..c2]`) e bloco `inicio ... fimalgoritmo`.
   - Expressões e operadores `.alg` (`<-`, `=`, `<>`, `\`, `^`, `mod`/`%`, `e`, `ou`, `xou`, `nao`) com precedência exata.
   - Estruturas de controle: `se/entao/senao/fimse`, `escolha/caso/outrocaso/fimescolha`, `para/de/ate/passo/faca/fimpara` (inclusive `passo` negativo), `enquanto/faca/fimenquanto`, `repita/ate`, `interrompa`.
   - Sub-rotinas: `procedimento` e `funcao` (`retorne`), passagem por valor e referência (`var`), e chamadas permissivas (`escreval`, `limpatela` com ou sem `()`).
   - Mensagens estruturadas de erro léxico/sintático em PT-BR com linha e coluna exatas.
2. **Motor de Execução, Builtins e Depurador:**
   - Inicialização automática padrão em `var` (`0`, `0.0`, `""`, `falso`).
   - Formatação de saída `escreva(valor:largura:decimais)` e `escreval(...)`.
   - Biblioteca padrão v0.1: matemáticas (`raizq`, `pot`, `abs`, `int`, `exp`, `sen`, `cos`, `tan`, `pi`, `rand`, `randi`) e texto (`compr`, `copia`, `pos`, `maiusc`, `minusc`, `carac`, `asc`, `numpcarac`, `caracpnum`).
   - Controle de execução: *trait* `IoHandler` desacoplada, *breakpoints*, execução passo a passo (`F8`), pausa, *snapshot* de escopo global + pilha atual, `cancel_flag` atômica e limite configurável de recursão (`500` na IDE / `100` padrão na CLI).
3. **CLI e Suíte Golden v0.1:**
   - Implementar `algoritmoslab-cli` com injeção de entradas simuladas para `leia()`.
   - Criar ~25 arquivos `.alg` de teste (incluindo `Windows-1252`, `UTF-8`, índices negativos, recursão, passagem por `var`, formatação `:5:2` e casos de erro esperados).

**Critério de pronto:** 100% dos testes unitários e da suíte *Golden* v0.1 passando com `cargo test`.

---

## Fase 2 — Host Desktop Tauri v2 e Ponte IPC

**Objetivo:** Conectar `algoritmoslab-core` ao ciclo de vida desktop com segurança, encoding e responsividade.

1. **Serviço de Arquivos e Encoding:**
   - Abrir `.alg` com detecção automática `UTF-8` / `Windows-1252` (`encoding_rs`) e normalização `CRLF`/`LF`.
   - Salvar em `UTF-8` por padrão e oferecer comando explícito para salvar em `Windows-1252` (compatibilidade com professores no Windows clássico).
2. **Executor de Sessão e Protocolo IPC:**
   - Rodar a VM em *thread* dedicada com `cancel_flag` atômica.
   - Agrupar eventos de saída (~16 ms) antes de emitir para a WebView.
   - Fluxo de `leia()` bloqueando a *thread* da VM de forma segura até resposta da UI (com validação e redigitação amigável).
   - Eventos tipados para mudança de linha atual, pausa em breakpoint, *snapshot* de variáveis e erro de execução.

**Critério de pronto:** Testes de integração em Rust validando execução contínua, `leia()`, pausa em breakpoint, atualização de variáveis e parada instantânea de loop infinito.

---

## Fase 3 — Interface React + CodeMirror 6 (`AlgoritmosLab` v0.1)

**Objetivo:** Entregar a primeira versão desktop instalável no Ubuntu com UI/UX limpa e familiar.

1. **Editor (CodeMirror 6):**
   - Realce de sintaxe Portugol (`.alg`), numeração de linhas, gutter clicável de *breakpoints*, destaque de linha em execução e linha de erro.
   - Autocomplete, auto-fechamento de blocos (`se` → `fimse`) e auto-indentação habilitados por padrão com toggle ("modo neutro/prova").
   - Novo arquivo pré-preenchido com esqueleto clássico (`algoritmo "semnome" ... var ... inicio ... fimalgoritmo`).
2. **Workspace da IDE (Inspirado na clareza do `portugol.dev`):**
   - Barra superior enxuta (`Novo`, `Abrir`, `Salvar`, `F9` Executar, `F8` Passo a Passo, `Pausar`, `Parar`, `Exemplos`, `Configurações`, `Tema Claro/Escuro`).
   - Painel lateral de Variáveis e Pilha (com visualização expansível de vetores e matrizes).
   - Console Integrado inferior redimensionável (`escreva`, entrada inline do `leia`, `limpatela`).
   - Galeria embutida de exemplos didáticos em português e aviso ao tentar fechar/trocar com alterações não salvas.
3. **Testes UI e Build Local Ubuntu:**
   - Testes com Vitest + Testing Library cobrindo máquina de estados da interface.
   - Gerar e instalar pacote `.deb` localmente no Ubuntu para validação manual real.

**Critério de pronto:** `AlgoritmosLab` v0.1 instalado via `.deb` no Ubuntu, executando e depurando arquivos `.alg` reais sem travamentos.

---

## Fase 4 — Recursos Avançados da Linguagem `.alg` (`v0.2`)

**Objetivo:** Completar o suporte aos recursos complementares e legados de Portugol estruturado (`.alg`).

1. **`registro` (tipos compostos):** definição de estruturas `tipo ... = registro ... fimregistro` e vetores de registros, com exibição estruturada no Painel de Variáveis.
2. **Comandos especiais e controle:** `aleatorio` (`on/off/min,max`), `timer` (`on/off/ms`), `cronometro` (`on/off`), `pausa`, `debug`, `eco` e `mudacor` (cores do console) + aviso seguro caso apareça comando externo (`dos`).
3. **Comando `arquivo` com sandbox:** suporte a leitura/escrita em arquivos de texto no padrão `.alg`, restrito à pasta do arquivo salvo (ou solicitando seleção explícita de pasta de trabalho se o algoritmo ainda não estiver salvo).
4. **Expansão da Suíte Golden:** +15–20 casos `.alg` cobrindo `registro`, `arquivo`, `aleatorio` e comandos especiais.

**Critério de pronto:** Suíte *Golden* completa (v0.1 + v0.2) e testes de integração verdes.

---

## Fase 5 — CI Multiplataforma e Documentação Open Source

**Objetivo:** Garantir releases reproduzíveis para Linux, Windows e macOS e preparar o projeto para contribuidores.

1. **GitHub Actions:** pipeline executando build frontend (`npm ci`, `npm test`, `npm run build`) antes das verificações `src-tauri`, `cargo clippy --locked`, `cargo test --locked`, suíte Golden e `tauri build` em `ubuntu-22.04` (`.deb`, `AppImage`), `windows-latest` (`.exe`/`.msi`) e `macos-latest` (`.dmg`), com `Cargo.lock` (workspace raiz e `src-tauri/`) versionados para builds reproduzíveis.
2. **Documentação:** `README.md` em português (instalação, atalhos, propósito educacional), `CONTRIBUTING.md` (como rodar testes e adicionar casos *Golden*) e registro explícito de comportamento e limites documentados para `.alg`.

**Critério de pronto:** Pipeline multiplataforma verde gerando artefatos instaláveis nos 3 sistemas operacionais.
