algoritmo "Logica"
var
   v: vetor[1..2] de inteiro
   i: inteiro
   a, b: logico
inicio
   v[1] <- 10
   v[2] <- 20
   i <- 5
   se (i <= 2) e (v[i] = 10) entao
      escreval("ERRO")
   senao
      escreval("CURTO_E_OK")
   fimse
   a <- VERDADEIRO
   b <- FALSO
   escreval(a xou b, " ", a xou a, " ", nao b)
fimalgoritmo
