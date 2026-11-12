algoritmo "RegistroSimples"
tipo
   TAluno = registro
      nome: caractere
      nota: real
      aprovado: logico
   fimregistro
var
   a, b: TAluno
inicio
   leia(a.nome, a.nota)
   a.aprovado <- a.nota >= 7
   b <- a
   b.nome <- "CopiaAlterada"
   escreval(a.nome, " ", a.nota:4:1, " ", a.aprovado)
   escreval(b.nome, " ", b.nota:4:1)
fimalgoritmo
