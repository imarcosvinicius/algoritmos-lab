# AlgoritmosLab — Documento de Design

> **Status:** Design Validado (Fase de Brainstorming Concluída)  
> **Licença:** GPL-3.0-only  
> **Escopo:** Fase 1 — IDE Desktop multiplataforma de Portugol estruturado (`.alg`)

---

## 1. Resumo de Entendimento (Understanding Summary)

- **O que está sendo construído:** O **AlgoritmosLab**, uma IDE desktop nativa, leve e multiplataforma para a linguagem e arquivos `.alg` em Portugol estruturado.
- **Por que existe:** Ferramentas educacionais clássicas de desktop eram proprietárias, restritas ao Windows (sem execução nativa no Ubuntu/Linux ou macOS) e com interface datada. O projeto busca democratizar o aprendizado de lógica de programação oferecendo execução fiel em qualquer sistema operacional com ótima UI/UX.
- **Para quem é:** Uso pessoal como prioridade inicial, projetado para atender estudantes e professores (instalação simples, interface acolhedora em português) e a comunidade open source.
- **Critério de sucesso (Fase 1):** Fidelidade completa de execução em Portugol estruturado (`.alg`, depuração passo a passo e tabela de variáveis), combinada com leveza, inicialização rápida e interface moderna inspirada na clareza ergonômica de ambientes web didáticos modernos (sem copiar), preservando o fluxo familiar de aprendizado.
- **Restrições-chave:** Aplicativo desktop 100% offline, zero telemetria, suporte transparente a arquivos em `Windows-1252`/`ISO-8859-1` e `UTF-8`, proteção contra travamentos (loops infinitos/recursão) e execução segura para acesso a disco (`arquivo`).
- **Não-objetivos explícitos (Non-goals):**
  - *Permanentes (fora da visão atual):* aplicativos mobile (Android/iOS), versão Web em navegador, contas/login/nuvem e recursos pesados de IDE profissional (plugins genéricos, Git integrado, edição colaborativa em tempo real).
  - *Adiado para fase futura (fora da Fase 1):* suporte a outros dialetos `.por`, mantido viável apenas pelo desacoplamento entre o núcleo interpretador e a interface.

---

## 2. Suposições e Requisitos Não Funcionais (Assumptions)

- **Sistemas-alvo:** Desktop Linux (Ubuntu como referência imediata), Windows e macOS.
- **Referência de linguagem:** Portugol estruturado clássico (`.alg`) como base para sintaxe, semântica, funções internas e comportamento de depuração.
- **Performance:** Inicialização rápida (~1–2s), baixo consumo de RAM em repouso, renderização fluida do console mesmo em laços intensos e interrupção instantânea pelo botão **Parar**.
- **Escala:** Uso local individual (1 usuário por máquina), algoritmos típicos acadêmicos (dezenas a poucos milhares de linhas) e vetores/matrizes de porte didático.
- **Segurança e Privacidade:** Funcionamento 100% offline, zero telemetria, limites seguros de profundidade de pilha/alocação e sandbox de diretório para o comando `arquivo`.
- **Confiabilidade:** Leitura/gravação sem corromper acentos (`Windows-1252` e `UTF-8`), normalização de quebras de linha (`CRLF`/`LF`) e aviso/proteção contra perda de alterações não salvas.
- **Manutenção e Propriedade:** Projeto open source sob **GPL-3.0**, mantido inicialmente pelo autor e estruturado com suíte automatizada de testes de paridade `.alg` e builds via CI.

---

## 3. Registro de Decisões (Decision Log)

| ID | Decisão | Alternativas Consideradas | Motivo da Escolha |
| :--- | :--- | :--- | :--- |
| **D1** | **Foco da Fase 1 em `.alg` (Portugol estruturado); `.por` adiado para fase futura** | Suportar `.alg` e `.por` simultaneamente na Fase 1; excluir `.por` permanentemente | Evita misturar duas gramáticas e runtimes distintos logo no início (YAGNI), mantendo o núcleo desacoplado para evolução futura. |
| **D2** | **UI/UX própria inspirada em ergonomia educacional moderna mantendo fluxo clássico** | Clonar visualmente ferramentas antigas; usar layout genérico de editor profissional | Democratiza o estudo com usabilidade moderna sem causar estranhamento a professores e alunos habituados ao formato `.alg`. |
| **D3** | **Arquitetura em Tauri v2 (`algoritmoslab-core` em Rust + WebView nativa do SO)** | Wails (Go + WebView); Electron (TS + Chromium); UI nativa em C++/Qt 6 | Melhor equilíbrio entre leveza real (sem embutir navegador), segurança/desempenho no interpretador, testes isolados sem UI e acessibilidade para contribuidores web. |
| **D4** | **Nome do projeto: AlgoritmosLab (`io.github.imarcosvinicius.AlgoritmosLab`)** | Usar nomes de softwares proprietários existentes; `AlgLivre`; `AlgLab` | Evita conflito de marca/pacotes com terceiros, reforça o caráter educacional/aberto e acomoda futuros dialetos. |
| **D5** | **Frontend em React + TypeScript + CodeMirror 6** | Svelte + CodeMirror 6; Monaco Editor | React amplia a base de contribuidores open source; CodeMirror 6 mantém a interface leve e lida nativamente com acentuação PT-BR. |
| **D6** | **Erros didáticos em português + guardrails de execução** | Replicar mensagens crípticas do original; executar sem sandbox/limites | Ajuda iniciantes a entender falhas e protege máquinas de estudo contra loops infinitos, estouro de pilha e acessos indevidos ao disco. |
| **D7** | **Licença open source GPL-3.0** | MIT; Apache-2.0 | Garante por *copyleft* que o projeto e eventuais versões derivadas distribuídas permaneçam sempre abertos e livres. |
| **D8** | **Entrega incremental: v0.1 (núcleo + IDE) → v0.2 (recursos complementares)** | Implementar 100% dos recursos antes da primeira versão | Viabiliza uso e validação rápidos no Ubuntu sem sacrificar a paridade completa na v0.2 (`registro`, `arquivo`, `timer`, `cronometro`, `aleatorio`, `eco`). |
| **D9** | **Console integrado, assistência ligada por padrão e esqueleto clássico** | Console em janela separada; editor neutro sem autocomplete | Alinha ergonomia moderna (`portugol.dev`) ao fluxo clássico, mantendo toggle de "modo prova/neutro" nas configurações. |
| **D10** | **Comportamentos ambíguos definidos para UX didática + compatibilidade** | Erro estrito no `leia()`; rejeitar chamadas sem `()`; pasta temporária silenciosa em `arquivo` | `leia()` inválido solicita redigitação amigável; `escreval`/`limpatela` aceitam com e sem `()`; `arquivo` exige salvar `.alg` ou escolher pasta de trabalho antes de rodar. |
| **D11** | **Ubuntu `.deb` prioritário, UI direto em PT-BR e id `io.github.imarcosvinicius.AlgoritmosLab`** | `AppImage` prioritário; sufixo `.app`; estrutura `i18n` desde o dia 1 | `.deb` instala nativamente no Ubuntu 24+ sem depender de `libfuse2`; evita conflito de pacotes e permite verificação automática no Flathub via GitHub; PT-BR direto mantém o código limpo. |

---

## 4. Design Final

### 4.1 Arquitetura Geral (3 Camadas)

1. **Núcleo do Interpretador (`algoritmoslab-core` — biblioteca pura em Rust)**
   - Implementa `lexer`, `parser`/AST, analisador semântico e máquina virtual (`VM`) de execução e depuração em Portugol estruturado (`.alg`).
   - Zero dependência de Tauri, janelas ou interface web, permitindo testes automatizados diretos em CLI e futura adição de um motor `.por` sem refazer a IDE.
2. **Host Desktop (Aplicação Tauri v2 em Rust)**
   - Gerencia janelas nativas, diálogos de arquivo, detecção/conversão de codificação (`Windows-1252` ↔ `UTF-8`), restrições de diretório para `arquivo` e empacotamento multiplataforma.
   - Executa cada algoritmo em uma *thread* dedicada com comunicação assíncrona via IPC/eventos para não bloquear a interface.
3. **Interface do Usuário (React + TypeScript + CodeMirror 6 na WebView nativa)**
   - Roda sobre `WebKitGTK` (Ubuntu/Linux), `WebView2` (Windows) e `WKWebView` (macOS).
   - Responsável por edição de código, atalhos familiares, console interativo, painel de variáveis e controles de depuração.

### 4.2 Componentes Principais

- **No `algoritmoslab-core`:**
  - *Lexer / Parser / AST:* *case-insensitive*, tolerante a acentos em palavras-chave, cobrindo `se/escolha`, `para/enquanto/repita`, `vetor/matriz`, `procedimento/funcao` (valor e `var`), `registro` e `arquivo`.
  - *VM / Depurador:* tabela de símbolos com escopos, pilha de chamadas, pontos de parada (*breakpoints*), execução contínua, passo a passo (`F8`), pausa e cancelamento atômico imediato.
  - *Biblioteca Padrão:* funções matemáticas (`raizq`, `int`, `randi`, `sen`, `cos`, `abs`, etc.), texto (`compr`, `copia`, `pos`, `maiusc`, `minusc`, `carac`, `asc`, `numpcarac`, `caracpnum`) e controle (`limpatela`, `timer`, `pausa`, `aleatorio`, `mudacor`).
- **No Host Tauri:**
  - *Executor de Sessão:* gerencia ciclo de vida da *thread* de execução, canal de bloqueio para `leia()` e *flag* atômica de parada.
  - *Serviço de Arquivos e Encoding:* lê/salva `.alg` detectando `UTF-8` e `Windows-1252`, normalizando `CRLF`/`LF`.
- **Na Interface (React + CodeMirror 6):**
  - *Barra Superior:* ações rápidas (`Novo`, `Abrir`, `Salvar`, `Executar F9`, `Passo a Passo F8`, `Pausar`, `Parar`, `Exemplos`, `Tema Claro/Escuro`).
  - *Editor:* CodeMirror 6 com destaque de sintaxe Portugol (`.alg`), numeração de linhas, destaque da linha atual em depuração e marcação de *breakpoints*.
  - *Painel de Variáveis:* tabela dinâmica (nome, tipo e valor) com inspeção expansível de vetores, matrizes e registros.
  - *Console Interativo:* saída de `escreva`/`escreval`, campo de entrada inline para `leia()` e suporte a `limpatela`.

### 4.3 Fluxo de Dados

1. **Abertura e Salvamento:** Ao abrir um `.alg`, o Host lê os bytes, detecta `UTF-8` ou aplica *fallback* para `Windows-1252` e entrega texto `UTF-8` ao editor. Ao salvar, usa `UTF-8` por padrão com opção explícita de salvar em `Windows-1252` para compatibilidade reversa com sistemas legados.
2. **Compilação e Execução:** O React envia código-fonte, modo (`contínuo` ou `passo a passo`) e *breakpoints* ao Host. O `algoritmoslab-core` gera a AST; erros sintáticos retornam imediatamente com linha/coluna antes da execução.
3. **Ciclo Interativo e Depuração:**
   - *Saída:* eventos de escrita são agrupados em lotes curtos (~16 ms) no Rust antes de ir para o React, evitando gargalo na WebView em laços rápidos.
   - *Entrada (`leia`):* a VM emite `AguardandoEntrada`, bloqueia sua *thread* em um canal e retoma após o usuário confirmar a entrada no console.
   - *Depuração e Parada:* a cada passo/breakpoint, a VM envia a linha atual e um *snapshot* dos escopos visíveis. O botão **Parar** ativa a `cancel_flag` atômica verificada pela VM, abortando loops infinitos instantaneamente.

### 4.4 Tratamento de Erros

- **Léxicos e Sintáticos:** retorno estruturado (linha, coluna, trecho e explicação didática em português) com destaque direto no editor e no console.
- **Runtime:** divisão por zero, índice fora dos limites, incompatibilidade de tipo, erro de assinatura em sub-rotinas ou recursão excessiva encerram a VM de forma limpa, mantendo visível o último estado das variáveis para diagnóstico.
- **Entrada (`leia`) e `arquivo`:** entradas incompatíveis exibem aviso claro em português e permitem redigitação sem perder a sessão (com alternância opcional para **"Leitura estrita no leia()"** nas Configurações); acessos inválidos ou fora do diretório permitido pelo `arquivo` retornam erro amigável.
- **Proteção de Recursos:** limite configurável de profundidade de recursão (`500` chamadas na IDE / `100` na CLI) e tamanho máximo de alocação (`1.000.000` elementos por vetor/matriz e `4.000.000` totais na VM) previnem travamentos e estouro de memória.

### 4.5 Casos de Borda e Recursos da Linguagem `.alg`

- Palavras-chave *case-insensitive* com ou sem acento (`inicio`/`início`, `entao`/`então`, `senao`/`senão`, `nao`/`não`, `funcao`/`função`, `ate`/`até`).
- Operadores clássicos: `<-`, `=`, `<>`, `\`, `^`, `mod`/`%`, `e`, `ou`, `xou`, `nao`.
- Inicialização automática de variáveis (`0`, `0.0`, `""`, `falso`).
- Faixas arbitrárias em vetores/matrizes (`vetor[1..10]`, `vetor[0..9]`, `vetor[-3..3]`).
- Laço `para` inclusivo com suporte a `passo` negativo e passagem por referência com `var`.
- Formatação numérica `escreva(valor:largura:decimais)`.
- Suporte a `aleatorio on/off/min,max` e `timer`.
- `mudacor` aplicado apenas às cores visuais do console da IDE; comandos de sistema/shell externo (`dos`) não são executados por segurança e emitem aviso.

### 4.6 Estratégia de Testes e Distribuição

- **Testes Unitários (`cargo test`):** cobertura de *lexer*, *parser*, precedência, escopos, vetores/matrizes, `registro` e funções nativas no `algoritmoslab-core`.
- **Suíte "Golden" de Conformidade (`.alg`):** coleção versionada de algoritmos clássicos (incluindo arquivos `Windows-1252` e `UTF-8`) executados por um *runner* CLI mínimo com simulação de `leia()` para validar saída exata sem UI.
- **Testes de UI (Vitest + Testing Library):** validação da máquina de estados do React (bloqueio em `leia()`, atualização de variáveis no `F8` e cancelamento de loop infinito).
- **CI e Empacotamento:** pipeline multiplataforma (GitHub Actions) rodando testes em Ubuntu, Windows e macOS e gerando `.deb`/`AppImage`, `.exe`/`.msi` e `.dmg`.

---

## 5. Riscos Reconhecidos e Mitigações

| Risco | Impacto | Mitigação |
| :--- | :--- | :--- |
| **Diferenças de `WebKitGTK` entre distros Linux** | Renderização CSS ou fontes ligeiramente diferentes no Ubuntu/derivados | Testar pacotes `.deb` e `AppImage` no Ubuntu desde cedo e evitar recursos CSS experimentais na UI. |
| **Diferenças sutis em apostilas clássicas de `.alg`** | Divergência pontual em arquivos legados de professores | Manter suíte *Golden* com exemplos reais variados e documentar casos ambíguos em `CONTRIBUTING.md`. |
| **Assinatura de binários (Windows/macOS)** | Avisos de SmartScreen/Gatekeeper para usuários finais | Automatizar builds no CI e documentar instruções claras de instalação/verificação para comunidade open source. |
| **Manutenção entre Rust e TypeScript** | Atraso ou erros na comunicação IPC | Manter o contrato de comandos/eventos Tauri pequeno, estável e versionado com tipos explícitos. |
