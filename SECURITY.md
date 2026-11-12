# Política de Segurança

## Versões Suportadas

Apenas a série de lançamento atual recebe correções de segurança:

| Versão | Suporte |
| :--- | :--- |
| `0.2.x` | Sim |
| `< 0.2` | Não |

## Modelo de Segurança e Sandbox

O **AlgoritmosLab** executa algoritmos em Portugol estruturado (`.alg`) localmente e 100% offline, com camadas de isolamento na VM (`algoritmoslab-core`) e no host Tauri:

- **Sandbox do comando `arquivo`:** leitura e gravação aceitam apenas caminhos relativos dentro da pasta do arquivo `.alg` salvo ou de uma pasta de trabalho explicitamente autorizada pelo usuário. Caminhos absolutos, travessia de diretórios (`..`), links simbólicos (inclusive em pastas intermediárias), arquivos não regulares e nomes reservados de dispositivos do Windows (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`) são bloqueados. Há ainda limites de tamanho (`2 MiB` para leitura; `10 MiB` ou `200.000` linhas para gravação).
- **Comando `dos` desativado:** instruções `dos` são aceitas pelo *parser* apenas para compatibilidade sintática e emitem aviso no console, sem executar comandos ou processos externos.
- **Proteção contra loops infinitos, recursão excessiva e OOM:** o motor conta com cancelamento atômico imediato, limites de profundidade de chamadas (`500` na IDE / `100` padrão na CLI) e blocos aninhados (`768`), tetos no *parser* e avaliador de expressões, limites de alocação para `vetor`/`matriz` (`1.000.000` de elementos por estrutura e `4.000.000` globais na VM) e limite de `16 MiB` para strings.

## Como Reportar Vulnerabilidades

Se você identificar uma vulnerabilidade de segurança (como escape da sandbox do comando `arquivo`, execução indevida ou negação de serviço), **não** abra uma *issue* pública antes da avaliação e correção.

Reporte de forma privada antes de qualquer divulgação pública utilizando **GitHub Security Advisories** (opção *Report a vulnerability* na aba **Security** do repositório) ou outro canal privado equivalente do projeto, informando:

- Versão afetada do AlgoritmosLab e sistema operacional.
- Descrição do impacto e passos mínimos para reprodução (ou arquivo `.alg` de exemplo).
- Eventuais recomendações de mitigação, caso identificadas.

Os mantenedores analisarão o relato, prepararão a correção e coordenarão a divulgação responsável após a disponibilização da atualização.
