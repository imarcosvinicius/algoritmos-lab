algoritmo "BubbleSort"
var
   v: vetor[1..5] de inteiro
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
   v[1] <- 5
   v[2] <- 1
   v[3] <- 4
   v[4] <- 2
   v[5] <- 3
   para i de 1 ate 4 faca
      para j de i + 1 ate 5 faca
         se v[i] > v[j] entao
            troca(v[i], v[j])
         fimse
      fimpara
   fimpara
   para i de 1 ate 5 faca
      escreva(v[i], " ")
   fimpara
fimalgoritmo
