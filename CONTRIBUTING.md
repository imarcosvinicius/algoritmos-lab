# Contribuindo com o AlgoritmosLab

Obrigado pelo interesse em contribuir com o **AlgoritmosLab**! Este guia resume a arquitetura do repositório, os comandos de verificação e as diretrizes da linguagem **Portugol estruturado (`.alg`)**.

---

## Estrutura do Projeto

```text
algoritmoslab/
├── crates/
│   ├── algoritmoslab-core/ # Biblioteca pura em Rust (lexer, parser, AST, VM, builtins, sandbox)
│   └── algoritmoslab-cli/  # Runner CLI para executar .alg no terminal e validar a suíte Golden
├── src-tauri/              # Host desktop Tauri v2 (I/O com detecção de encoding, IPC, SessionManager)
├── src/                    # Interface React 19 + TypeScript + CodeMirror 6 + Vitest
├── tests/golden/           # Suíte de regressão e conformidade (.alg / .in / .out / .err)
├── docs/                   # Documento de design e plano de implementação
└── .github/workflows/      # Pipeline CI multiplataforma (Linux, Windows e macOS)
```

Mantenha o núcleo `crates/algoritmoslab-core` completamente desacoplado de Tauri e da UI web para preservar a testabilidade via CLI e viabilizar futuras extensões.

---

## Como Rodar Testes e Linters

Execute toda a bateria de verificação antes de abrir um *Pull Request*. Tanto `Cargo.lock` (raiz) quanto `src-tauri/Cargo.lock` são versionados para garantir builds reproduzíveis com `--locked`.

> **Importante:** execute o build do frontend (`npm run build`) **antes** de compilar ou testar `src-tauri/`, pois a macro `tauri::generate_context!()` exige que `../dist` exista em tempo de compilação (especialmente em *checkouts* limpos e no CI).

```bash
# 1. Frontend React / CodeMirror 6 (dependências + Vitest + TypeScript + gera dist/)
npm ci
npm test
npm run build

# 2. Núcleo algoritmoslab-core + algoritmoslab-cli (Clippy estrito + unitários + suíte Golden)
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
cargo run --locked -q -p algoritmoslab-cli -- --golden tests/golden

# 3. Host Tauri v2 (Clippy estrito + testes de integração da sessão IPC; requer dist/)
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

---

## Como Adicionar Casos na Suíte *Golden*

Os testes ficam em `tests/golden/` e são executados automaticamente tanto pelo `cargo test --workspace --locked` (`algoritmoslab_core::golden` e `crates/algoritmoslab-core/tests/golden_tests.rs`) quanto pelo `algoritmoslab-cli -- --golden tests/golden`.

Convenção de arquivos por caso `<NN>_<nome_descritivo>`:

- `<caso>.alg`: código-fonte em Portugol estruturado (`UTF-8` ou `Windows-1252`).
- `<caso>.in` *(opcional)*: uma linha por entrada esperada em `leia()`.
- `<caso>.out`: saída exata esperada no stdout (se o programa deve concluir com sucesso).
- `<caso>.err`: trecho obrigatório da mensagem de erro esperada (se o caso valida erro léxico, sintático, semântico ou de runtime).

### Diretivas e *Fixtures* no `algoritmoslab_core::golden`

- **Diretivas de cabeçalho:** o *runner* compartilhado `algoritmoslab_core::golden` aceita comentários no topo do `.alg` (antes da linha `algoritmo`) no formato:
  `// golden: rng_seed=<u64> strict_read=true|false`
  Isso permite tornar determinísticos casos com `aleatorio`/`rand`/`randi` e validar falhas no modo de leitura estrita.
- **Isolamento de *fixtures*:** arquivos auxiliares que não usam extensão de caso (isto é, extensões diferentes de `.alg`, `.in`, `.out` e `.err`, como `33_dados.txt`) são copiados automaticamente para um diretório *sandbox* temporário isolado a cada execução de caso no `algoritmoslab_core::golden`, evitando poluir `tests/golden/`. Ainda assim, mantenha prefixos numéricos únicos nos nomes das *fixtures* para compatibilidade com execuções diretas via `algoritmoslab-cli -- --golden`.

---

## Comportamento e Limites Documentados

O objetivo do AlgoritmosLab é oferecer **máxima compatibilidade prática** com apostilas e arquivos `.alg` reais, aplicando proteções modernas de segurança, previsibilidade e didática. As regras e limites documentados são:

1. **Comando `dos` desativado por segurança:**
   - Em ambientes legados do Windows, `dos` executava comandos do prompt. No AlgoritmosLab, a instrução é aceita pelo *parser* para não quebrar scripts existentes, porém não executa processos externos e emite aviso no console.
2. **Comando `arquivo` restrito por *sandbox*, troca sequencial e criação *lazy*:**
   - Aceita leitura (`UTF-8` ou `Windows-1252`, voltando ao teclado ao atingir EOF) e gravação apenas de caminhos relativos dentro da pasta do `.alg` salvo ou de uma pasta autorizada pelo usuário.
   - Permite no máximo 1 diretiva `arquivo` no cabeçalho declarativo e trocas sequenciais de `arquivo` dentro do bloco `inicio` (realizando *flush* do escritor anterior antes de abrir o próximo).
   - Escopo e fluxo global: a diretiva `arquivo` é proibida dentro da seção `var` local de `procedimento`/`funcao`, enquanto trocas sequenciais de `arquivo "..."` dentro de blocos `inicio` afetam o fluxo de arquivo global da execução (após realizar *flush* de qualquer escritor anterior).
   - Criação *lazy*: quando o arquivo ainda não existe, ele só é criado em disco no primeiro `leia()` (evitando arquivos vazios se nenhuma leitura ocorrer).
   - Bloqueia caminhos absolutos, `..`, nomes reservados de dispositivo Windows (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`), links simbólicos (inclusive em pastas intermediárias) e arquivos não regulares.
   - Limites de segurança do `arquivo`: `2 MiB` para leitura e `10 MiB` ou `200.000` linhas para gravação.
3. **Validação amigável no `leia()` (com toggle para modo estrito) e formato de entrada:**
   - Por padrão, valores inválidos para `inteiro`, `real` ou `logico` exibem orientação clara em português e pedem redigitação sem encerrar o programa do aluno. É possível ativar **"Leitura estrita no leia()"** nas Configurações quando se deseja falha imediata.
   - `leia()` para `real` e `caracpnum()` aceitam vírgula decimal PT-BR (quando única e sem ponto, ex.: `"12,5"`), rejeitando valores não finitos (`NaN`, `inf`).
   - `leia()` para `logico` aceita `verdadeiro`, `v`, `true`, `1` e `falso`, `f`, `false`, `0` (sem distinção de maiúsculas ou acentos).
4. **Limites de proteção contra travamento e estouro de recursos:**
   - **Recursão e aninhamento em *runtime*:** profundidade máxima de chamadas de `500` na IDE e `100` por padrão na CLI (configurável via `ExecOptions`), além de limite combinado de `768` blocos aninhados ativos na VM.
   - ***Parser* e avaliador:** profundidade sintática máxima de `256` níveis, máximo de `16.384` nós por expressão e profundidade máxima de avaliação de `256` (com teto de `131.072` passos por expressão).
   - **Memória da VM e strings:** limite de `1.000.000` elementos folha por `vetor`/`matriz`, orçamento global de `4.000.000` elementos alocados simultaneamente na VM e máximo de `16 MiB` por string em concatenações/operações de texto.
   - **Formatação, `.alg` e `timer`:** largura `<= 4096` e casas decimais `<= 20` no `escreva`/`escreval`; arquivos `.alg` limitados a `4 MiB` tanto na IDE quanto na `algoritmoslab-cli`; atraso do `timer` limitado a no máximo `10.000 ms` por comando (`timer on` usa `500 ms`).
5. **Cores do `mudacor` aplicadas apenas ao console integrado:**
   - Alteram visualmente a área do console (e não a janela inteira da IDE), permanecendo ativas após `limpatela` e retornando ao tema da IDE no início de uma nova execução.
6. **Sub-rotinas, sintaxe permissiva e identificadores contextuais:**
   - `escreval`, `limpatela`, `pausa`, `aleatorio`, `debug`, `pi`, `rand` e procedimentos/funções do usuário sem parâmetros aceitam uso com ou sem parênteses `()`.
   - Já `escreva(...)` exige parênteses, e `timer` exige `on`, `off` ou duração numérica em milissegundos (limitada a `10.000 ms`).
   - `matriz` e `div` são tratados contextualmente (não são palavras reservadas globais): variáveis chamadas `matriz` e `div` funcionam livremente, enquanto `m: matriz[...]` e `a div b` continuam aceitos contextualmente.
   - Passagem direta de `vetor` e `registro` por valor ou por referência (`var`, além de campos de registro e posições de vetor) e declaração de `registro` *inline* diretamente na seção `var`.
   - `escolha` aceita intervalos tanto com `caso a..b` quanto com `caso a ate b`.
   - `exp` aceita 1 argumento ($e^x$) ou 2 argumentos (`exp(base, expoente)`), além de `pot(base, exp)`.
7. **Precedência, formatação, Unicode, `aleatorio` e pós-`fimalgoritmo`:**
   - O operador de potência `^` é associativo à direita (`2 ^ 3 ^ 2 = 512`).
   - `escreva(5)` / `escreval(5)` imprimem números positivos sem espaço em branco à esquerda.
   - `aleatorio` também gera valores booleanos (`VERDADEIRO`/`FALSO`) para variáveis `logico` (extensão didática), além de inteiros, reais (arredondados a 2 casas decimais) e strings de 5 letras maiúsculas (`A`–`Z`).
   - Funções de texto (`asc`, `carac`, `compr`, `copia`, `pos`) operam por caractere Unicode (`UTF-8`) em vez de bytes `CP1252` puros.
   - Tokens executáveis após `fimalgoritmo` geram erro sintático (apenas comentários `//` e separadores como quebras de linha e `;` são permitidos após `fimalgoritmo`).

Ao propor mudanças na gramática ou na semântica da VM, inclua ao menos um caso *Golden* correspondente e atualize esta lista caso alguma diferença comportamental nova seja introduzida.

---

## Licença

Ao contribuir com código, testes ou documentação para este repositório, você concorda que suas contribuições serão licenciadas sob **GPL-3.0-only** ([LICENSE](./LICENSE)).
