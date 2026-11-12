algoritmo "VarMatrizEDiv"
var
   matriz: vetor[1..2, 1..2] de inteiro
   m2: matriz[1..2, 1..2] de inteiro
   div: real
inicio
   matriz[1, 1] <- 7 div 2
   matriz[2, 2] <- 19 div 4
   m2[1, 2] <- matriz[1, 1] + matriz[2, 2]
   m2[2, 1] <- m2[1, 2] div 2
   div <- (7 div 2) + 0.5
   escreval(matriz[1, 1], "|", matriz[2, 2], "|", m2[1, 2], "|", m2[2, 1], "|", div:3:1)
fimalgoritmo
