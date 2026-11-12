algoritmo "ArquivoSequencialInicio"
var
   nome, lido: caractere
   nota, lida_nota: real
inicio
   arquivo "55_seq_a.txt"
   leia(nome, nota)
   arquivo "55_seq_a.txt"
   leia(lido, lida_nota)
   arquivo("55_seq_b.txt")
   leia(nome)
   escreval(lido, ":", lida_nota:3:1, ":", nome)
fimalgoritmo
