algoritmo "AliasingVarSubelementos"
tipo
   TAluno = registro
      nome: caractere
      nota: real
   fimregistro
var
   v: vetor[1..3] de inteiro
   turma: vetor[1..2] de TAluno
   i: inteiro

procedimento troca(var a, b: inteiro)
var
   aux: inteiro
inicio
   aux <- a
   a <- b
   b <- aux
fimprocedimento

procedimento p(var x: inteiro)
inicio
   v[1] <- 20
   escreval("dentro_p:", x)
   x <- x + 5
fimprocedimento

procedimento p2(var b: inteiro)
inicio
   b <- b * 3
fimprocedimento

procedimento p1(var a: inteiro)
inicio
   a <- a + 2
   p2(a)
fimprocedimento

procedimento q2(var n: real)
inicio
   n <- n + 1.5
fimprocedimento

procedimento q(var r: TAluno)
inicio
   r.nome <- "Ana"
   q2(r.nota)
fimprocedimento

inicio
   v[1] <- 10
   v[2] <- 4
   v[3] <- 99
   i <- 3

   troca(v[i], v[i])
   escreval("troca_mesmo:", v[3])

   p(v[1])
   escreval("apos_p:", v[1])

   p1(v[2])
   escreval("encadeado:", v[2])

   turma[1].nota <- 7.0
   q(turma[1])
   escreval("registro:", turma[1].nome, "=", turma[1].nota:3:1)
fimalgoritmo
