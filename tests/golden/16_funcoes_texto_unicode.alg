algoritmo "Texto"
var
   s: caractere
inicio
   s <- "Ação"
   escreval(compr(s), " ", copia(s, 2, 2), " ", pos("ão", s))
   escreval(maiusc("abc"), " ", minusc("XYZ"))
   escreval(asc("A"), " ", carac(66), " ", numpcarac(42), " ", caracpnum("12.5") * 2)
fimalgoritmo
