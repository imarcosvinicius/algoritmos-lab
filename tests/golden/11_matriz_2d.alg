algoritmo "Matriz"
var
   m: vetor[1..2, 1..3] de inteiro
   i, j: inteiro
inicio
   para i de 1 ate 2 faca
      para j de 1 ate 3 faca
         m[i, j] <- i * 10 + j
      fimpara
   fimpara
   escreval(m[1, 3], " ", m[2, 2])
fimalgoritmo
