algoritmo "EscolhaCasoIntervaloPontoPonto"
var
   k, n: inteiro
   letra: caractere
inicio
   para k de 1 ate 5 faca
      leia(n)
      escolha n
         caso 0..10
            escreval("zero_a_dez")
         caso 11, 12
            escreval("onze_ou_doze")
         outrocaso
            escreval("fora_faixa")
      fimescolha
   fimpara

   para k de 1 ate 3 faca
      leia(letra)
      escolha letra
         caso "A".."Z"
            escreval("maiuscula")
         caso "a".."z"
            escreval("minuscula")
         outrocaso
            escreval("outro")
      fimescolha
   fimpara
fimalgoritmo
