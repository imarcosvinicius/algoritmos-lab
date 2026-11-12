algoritmo "RegistroPorReferencia"
tipo
   TItem = registro
      titulo: caractere
      pontos: inteiro
   fimregistro
var
   lista: vetor[1..2] de TItem

procedimento adicionar(var n: inteiro; delta: inteiro)
inicio
   n <- n + delta
fimprocedimento

procedimento renomear(var item: TItem; novo: caractere)
inicio
   item.titulo <- novo
fimprocedimento

inicio
   lista[1].titulo <- "A"
   lista[1].pontos <- 10
   lista[2].titulo <- "B"
   lista[2].pontos <- 20

   adicionar(lista[1].pontos, 5)
   renomear(lista[2], "Beta")
   escreval(lista[1].titulo, "=", lista[1].pontos)
   escreval(lista[2].titulo, "=", lista[2].pontos)
fimalgoritmo
