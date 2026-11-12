algoritmo "Recursao"
funcao fatorial(n: inteiro): inteiro
inicio
   se n <= 1 entao
      retorne 1
   senao
      retorne n * fatorial(n - 1)
   fimse
fimfuncao

funcao fib(n: inteiro): inteiro
inicio
   se n <= 1 entao
      retorne n
   fimse
   retorne fib(n - 1) + fib(n - 2)
fimfuncao

inicio
   escreval(fatorial(5), " ", fib(7))
fimalgoritmo
