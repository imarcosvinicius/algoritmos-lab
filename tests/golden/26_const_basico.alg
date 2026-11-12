algoritmo "ConstantesBasico"
const
   MAX = 3
   TAXA = 1.5
   TITULO = "Resumo"
   ATIVO = verdadeiro
var
   v: vetor[1..MAX] de real
   i: inteiro
inicio
   escreval(TITULO, " (", ATIVO, ")")
   para i de 1 ate MAX faca
      v[i] <- i * TAXA
      escreval(v[i]:4:1)
   fimpara
fimalgoritmo
