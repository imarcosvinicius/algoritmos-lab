algoritmo "VetorRegistrosAninhados"
const
   TOTAL = 2
tipo
   TEndereco = registro
      cidade: caractere
      numero: inteiro
   fimregistro
   TPessoa = registro
      nome: caractere
      notas: vetor[1..2] de real
      end: TEndereco
   fimregistro
var
   turma: vetor[1..TOTAL] de TPessoa
   i: inteiro
   media: real
inicio
   turma[1].nome <- "Ana"
   turma[1].notas[1] <- 8
   turma[1].notas[2] <- 9
   turma[1].end.cidade <- "Recife"
   turma[1].end.numero <- 100

   turma[2].nome <- "Bruno"
   turma[2].notas[1] <- 6.5
   turma[2].notas[2] <- 7.5
   turma[2].end.cidade <- "Curitiba"
   turma[2].end.numero <- 42

   para i de 1 ate TOTAL faca
      media <- (turma[i].notas[1] + turma[i].notas[2]) / 2
      escreval(turma[i].nome, " - ", turma[i].end.cidade, ",", turma[i].end.numero, " -> ", media:4:1)
   fimpara
fimalgoritmo
