import { AlgorithmExample } from "../types/ide";

export const DIDACTIC_EXAMPLES: AlgorithmExample[] = [
  {
    id: "ola-mundo",
    title: "Olá, Mundo!",
    level: "Iniciante",
    description:
      "Estrutura mínima de um algoritmo em Portugol estruturado com declaração de variável de texto, leitura e saída formatada.",
    tags: ["escreval", "leia", "caractere"],
    source: `algoritmo "OlaMundo"
// Primeiro algoritmo no AlgoritmosLab
var
   nome: caractere
inicio
   escreval("=== Bem-vindo ao AlgoritmosLab ===")
   escreva("Qual é o seu nome? ")
   leia(nome)
   escreval("Olá, ", nome, "! Bons estudos com algoritmos.")
fimalgoritmo
`,
  },
  {
    id: "media-condicional",
    title: "Média e Situação do Aluno",
    level: "Iniciante",
    description:
      "Lê duas notas reais, calcula a média, formata com 2 casas decimais (:4:2) e decide aprovação via se/senao.",
    tags: ["real", "se/senao", ":4:2"],
    source: `algoritmo "MediaCondicional"
var
   n1, n2, media: real
inicio
   escreva("Digite a primeira nota: ")
   leia(n1)
   escreva("Digite a segunda nota: ")
   leia(n2)

   media <- (n1 + n2) / 2
   escreval("Média final: ", media:4:2)

   se media >= 7 entao
      escreval("Situação: Aprovado")
   senao
      se media >= 5 entao
         escreval("Situação: Recuperação")
      senao
         escreval("Situação: Reprovado")
      fimse
   fimse
fimalgoritmo
`,
  },
  {
    id: "tabuada-lacos",
    title: "Tabuada com Laço Para",
    level: "Estruturas",
    description:
      "Demonstra validação de entrada com repita/ate e geração da tabuada usando o laço para/fimpara.",
    tags: ["repita/ate", "para", "laços"],
    source: `algoritmo "Tabuada"
var
   num, i: inteiro
inicio
   repita
      escreva("Digite um número positivo para ver a tabuada: ")
      leia(num)
   ate num > 0

   escreval("Tabuada do ", num, ":")
   para i de 1 ate 10 faca
      escreval(num, " x ", i:2, " = ", (num * i):3)
   fimpara
fimalgoritmo
`,
  },
  {
    id: "bubblesort-vetor",
    title: "Ordenação BubbleSort (Vetor)",
    level: "Sub-rotinas",
    description:
      "Ordena um vetor de inteiros usando procedimento com parâmetros por referência (var). Ótimo para testar breakpoints e expansão de vetores!",
    tags: ["vetor", "procedimento", "var"],
    source: `algoritmo "BubbleSort"
var
   v: vetor[1..6] de inteiro
   i, j: inteiro

procedimento troca(var a, b: inteiro)
var
   aux: inteiro
inicio
   aux <- a
   a <- b
   b <- aux
fimprocedimento

inicio
   v[1] <- 42
   v[2] <- 15
   v[3] <- 8
   v[4] <- 23
   v[5] <- 4
   v[6] <- 16

   para i de 1 ate 5 faca
      para j de 1 ate 6 - i faca
         se v[j] > v[j + 1] entao
            troca(v[j], v[j + 1])
         fimse
      fimpara
   fimpara

   escreval("Vetor ordenado:")
   para i de 1 ate 6 faca
      escreva(v[i], " ")
   fimpara
   escreval("")
fimalgoritmo
`,
  },
  {
    id: "fatorial-recursivo",
    title: "Fatorial Recursivo (Pilha de Chamadas)",
    level: "Sub-rotinas",
    description:
      "Usa função recursiva com retorne para demonstrar a pilha de chamadas e variáveis locais durante o passo a passo (F8).",
    tags: ["funcao", "retorne", "recursão"],
    source: `algoritmo "FatorialRecursivo"
var
   n, resultado: inteiro

funcao fatorial(k: inteiro): inteiro
inicio
   se k <= 1 entao
      retorne 1
   senao
      retorne k * fatorial(k - 1)
   fimse
fimfuncao

inicio
   escreva("Calcular fatorial de (0 a 12): ")
   leia(n)
   se (n < 0) ou (n > 12) entao
      escreval("Informe um valor entre 0 e 12.")
   senao
      resultado <- fatorial(n)
      escreval(n, "! = ", resultado)
   fimse
fimalgoritmo
`,
  },
  {
    id: "matriz-diagonal",
    title: "Matriz 3x3 e Soma da Diagonal",
    level: "Estruturas",
    description:
      "Preenche uma matriz 3x3, exibe em formato tabular e soma os elementos da diagonal principal.",
    tags: ["vetor", "matriz 2D", "para aninhado"],
    source: `algoritmo "MatrizDiagonal"
var
   m: vetor[1..3, 1..3] de inteiro
   l, c, somaDiag: inteiro
inicio
   somaDiag <- 0
   para l de 1 ate 3 faca
      para c de 1 ate 3 faca
         m[l, c] <- (l - 1) * 3 + c
         se l = c entao
            somaDiag <- somaDiag + m[l, c]
         fimse
      fimpara
   fimpara

   escreval("Matriz preenchida:")
   para l de 1 ate 3 faca
      para c de 1 ate 3 faca
         escreva(m[l, c]:4)
      fimpara
      escreval("")
   fimpara
   escreval("Soma da diagonal principal: ", somaDiag)
fimalgoritmo
`,
  },
  {
    id: "cadastro-registros",
    title: "Cadastro com Registro e Constante",
    level: "Estruturas",
    description:
      "Usa const, tipo composto (registro ... fimregistro) e vetor de registros para cadastrar alunos e expandir campos no Painel de Variáveis.",
    tags: ["const", "registro", "vetor de registro"],
    source: `algoritmo "CadastroAlunos"
const
   TOTAL = 3
tipo
   TAluno = registro
      nome: caractere
      nota1, nota2, media: real
   fimregistro
var
   turma: vetor[1..TOTAL] de TAluno
   i: inteiro
inicio
   turma[1].nome <- "Ana"
   turma[1].nota1 <- 8.5
   turma[1].nota2 <- 9.5

   turma[2].nome <- "Bruno"
   turma[2].nota1 <- 6.0
   turma[2].nota2 <- 7.0

   turma[3].nome <- "Carla"
   turma[3].nota1 <- 9.0
   turma[3].nota2 <- 10.0

   escreval("=== Boletim da Turma ===")
   para i de 1 ate TOTAL faca
      turma[i].media <- (turma[i].nota1 + turma[i].nota2) / 2
      escreval(turma[i].nome, " -> Média: ", turma[i].media:4:2)
   fimpara
fimalgoritmo
`,
  },
  {
    id: "escolha-passo",
    title: "Menu com Escolha e Laço Regressivo",
    level: "Estruturas",
    description:
      "Demonstra seleção múltipla com escolha/caso/outrocaso e contagem regressiva usando passo negativo no laço para.",
    tags: ["escolha", "caso", "passo"],
    source: `algoritmo "EscolhaEPasso"
var
   opcao, i: inteiro
inicio
   escreval("1 - Contagem progressiva (passo 2)")
   escreval("2 - Contagem regressiva (passo -1)")
   escreva("Escolha uma opção (1 ou 2): ")
   leia(opcao)

   escolha opcao
      caso 1
         para i de 0 ate 10 passo 2 faca
            escreva(i, " ")
         fimpara
         escreval("")
      caso 2
         para i de 5 ate 1 passo -1 faca
            escreva(i, " ")
         fimpara
         escreval("Fogo!")
      outrocaso
         escreval("Opção inválida.")
   fimescolha
fimalgoritmo
`,
  },
];
