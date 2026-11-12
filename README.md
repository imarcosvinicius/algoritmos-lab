# AlgoritmosLab

**AlgoritmosLab** é uma IDE desktop aberta, leve e multiplataforma (Linux, Windows e macOS) para programação em **Portugol estruturado** (`.alg`).

Projetada para democratizar o ensino e a prática de lógica de programação fora do Windows sem abrir mão da compatibilidade com materiais clássicos de aula — funcionando **100% offline**, sem telemetria e sob licença **GPL-3.0-only**.

---

## Principais Recursos

### Linguagem `.alg` & Motor (`algoritmoslab-core`)
- **Palavras-chave flexíveis:** *case-insensitive* e tolerante a acentos (`inicio`/`início`, `entao`/`então`, `senao`/`senão`, `nao`/`não`, `funcao`/`função`, `ate`/`até`, `aleatorio`/`aleatório`, `cronometro`/`cronômetro`).
- **Tipos e declarações:** `const`, `tipo ... = registro ... fimregistro` (inclusive registros aninhados e inline em `var`), `var` com inicialização padrão (`0`, `0.0`, `""`, `falso`), `inteiro`, `real`, `caractere`/`caracter`/`literal`, `logico`, `vetor[min..max]` / `vetor[r1..r2, c1..c2]` (com `matriz[r1..r2, c1..c2]` aceito contextualmente por retrocompatibilidade, mantendo `matriz` liberado para nomes de variáveis; inclui índices negativos e constantes nos limites).
- **Operadores e controle:** `<-`, `=`, `<>`, `\`/`div`, `^`, `mod`/`%`, `e`, `ou`, `xou`, `nao`; `se/entao/senao/fimse`, `escolha/caso/outrocaso/fimescolha` (com intervalos `..` ou `ate`), `para/de/ate/passo/faca/fimpara`, `enquanto/faca/fimenquanto`, `repita/ate`, `interrompa`.
- **Sub-rotinas:** `procedimento` e `funcao` (`retorne`), passagem direta de tipos primitivos, `vetor` e `registro` por valor ou por referência (`var`, inclusive campos de registro e posições de vetor), além de recursão protegida contra estouro de pilha.
- **Biblioteca padrão:** matemáticas (`abs`, `int`, `quad`, `raizq`, `pot`, `exp`, `log`, `logn`, `pi`, `sen`, `cos`, `tan`, `cotan`, `arcsen`, `arccos`, `arctan`, `grauprad`, `radpgrau`, `rand`, `randi`) e texto (`compr`, `copia`, `pos`, `maiusc`, `minusc`, `carac`, `asc`, `numpcarac`, `caracpnum`).
- **Comandos clássicos especiais:** `escreva`/`escreval` (com formatação `:largura:decimais`), `leia`, `limpatela`, `aleatorio`, `timer`, `cronometro`, `pausa`, `debug`, `eco`, `mudacor` e `arquivo` com *sandbox*.

### Interface Educacional (React + CodeMirror 6 + Tauri v2)
- **Editor moderno e acessível:** numeração de linhas, destaque de sintaxe Portugol (`.alg`), *gutter* clicável de *breakpoints*, destaque de linha atual/erro, autocomplete e fechamento automático de blocos.
- **Modo Prova / Editor Neutro:** botão rápido na barra superior e nas Configurações para desativar autocomplete e auto-fechamento em avaliações.
- **Depurador integrado:** execução contínua (`F9`), passo a passo / entrar (`F8`), pular sub-rotina (`Shift+F8`), pausa, interrupção instantânea de loops infinitos, pilha de chamadas e Painel de Variáveis com expansão de vetores, matrizes e registros.
- **Compatibilidade real de arquivos:** abre `.alg` em `UTF-8` ou `Windows-1252` (*fallback* automático) e oferece opção dedicada para salvar cópia em `Windows-1252` (para uso em ambientes clássicos do Windows).
- **Galeria de exemplos e temas:** exemplos didáticos em português prontos para estudo e temas **Claro** e **Escuro**.

---

## Atalhos de Teclado

| Atalho | Ação |
| :--- | :--- |
| `F9` | Executar / Continuar execução |
| `F8` | Iniciar ou avançar passo a passo (Entrar em sub-rotina) |
| `Shift + F8` | Pular sub-rotina (*Step Over*) |
| `Ctrl + B` | Alternar *breakpoint* na linha atual do editor |
| `Ctrl + S` | Salvar algoritmo |
| `Ctrl + O` | Abrir arquivo `.alg` |
| `Ctrl + N` | Novo algoritmo |
| `Esc` | Fechar janela de Exemplos ou Configurações |

---

## Sandbox de Segurança (`arquivo` e `dos`)

- **Comando `arquivo "nome.txt"`:** por segurança, aceita apenas caminhos relativos dentro da **pasta do `.alg` salvo**. Caso o algoritmo ainda não tenha sido salvo, a IDE solicita que você salve ou selecione explicitamente uma **pasta de trabalho autorizada** (botão **Pasta arquivo** no cabeçalho do console). Caminhos absolutos, `..` e links simbólicos são bloqueados.
- **Comando `dos`:** desativado por segurança; exibe aviso no console integrado sem executar processos externos.

---

## Instalação e Build a partir do Código-Fonte

### Instaladores Pré-compilados (Usuários Finais)

Para uso direto sem compilar o código-fonte, baixe os pacotes pré-compilados na página de **GitHub Releases** (gerados por tag `v*`) ou nos artefatos recentes do **GitHub Actions**:

- **Linux (qualquer distribuição):** `.flatpak` (instale com `flatpak install --user AlgoritmosLab-x86_64.flatpak`) ou `.AppImage`; manifesto Flathub completo em [packaging/flatpak/README.md](./packaging/flatpak/README.md)
- **Ubuntu / Debian:** `.deb`
- **Windows:** `*-setup.exe` (instalador NSIS), `AlgoritmosLab-portable-win64.exe` ou `.msi`
- **macOS:** `.dmg`

### Pré-requisitos (Build Local)

- **Rust** estável (`rustup`, `cargo`, `clippy`)
- **Node.js** 22+ / 24+ e `npm`
- **No Ubuntu / Debian (dependências de sistema para Tauri v2):**
  ```bash
  sudo apt-get update
  sudo apt-get install -y \
    libwebkit2gtk-4.1-dev \
    build-essential \
    curl wget file \
    libxdo-dev \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    patchelf
  ```

### Executar em Desenvolvimento

```bash
npm install
npx tauri dev
```

### Gerar Pacote Instalável (ex.: `.deb` no Ubuntu)

Use `--locked` nos comandos Cargo e Tauri para respeitar os arquivos `Cargo.lock` versionados (raiz e `src-tauri/Cargo.lock`) e garantir builds reproduzíveis:

```bash
npm ci
npm run build
npx tauri build --bundles deb -- --locked
```

O pacote gerado fica em `src-tauri/target/release/bundle/deb/`. Para instalar localmente no Ubuntu (resolvendo dependências de sistema automaticamente):

```bash
sudo apt install ./src-tauri/target/release/bundle/deb/AlgoritmosLab_*.deb
```

### Usar apenas o Executor CLI

```bash
cargo run --locked -q -p algoritmoslab-cli -- caminho/para/algoritmo.alg
```

---

## Documentação Complementar e Contribuições

- Arquitetura, decisões de projeto, suíte de testes *Golden* e registro de divergências documentadas para a linguagem `.alg` estão descritos em [CONTRIBUTING.md](./CONTRIBUTING.md), [docs/design.md](./docs/design.md) e [docs/implementation-plan.md](./docs/implementation-plan.md).
- Contribuições são bem-vindas! Veja [CONTRIBUTING.md](./CONTRIBUTING.md) para saber como rodar todos os testes e adicionar novos casos `.alg`.

---

## Licença

Distribuído sob a licença **GNU General Public License v3.0 (`GPL-3.0-only`)**. Veja [LICENSE](./LICENSE) para detalhes.
