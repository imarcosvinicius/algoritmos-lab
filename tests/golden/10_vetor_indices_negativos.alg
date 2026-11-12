algoritmo "VetorNegativo"
var
   v: vetor[-2..2] de inteiro
   i, soma: inteiro
inicio
   para i de -2 ate 2 faca
      v[i] <- i * 10
   fimpara
   soma <- 0
   para i de -2 ate 2 faca
      soma <- soma + v[i]
   fimpara
   escreval(v[-2], " ", v[2], " ", soma)
fimalgoritmo
