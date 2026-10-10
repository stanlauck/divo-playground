VAR score = 0
-> start

=== start ===
Hello there. #greeting
* [Go north] -> north
* Go south
    You went south.
    -> END

=== north ===
= entry
You are north. {score > 0: You win.|Keep going.}
~ score = score + 1
-> DONE
