# CHARACTER_REPLACE

Identity replacement swaps ONLY:

- anchor tokens (`official style, <name> ...`) — replace the full anchor variant
- inherent hair / eye / body traits of the old character
- the old character's default outfit tokens
- the project/panel title character name

Never touch: camera terms (pov, dutch angle, cowboy shot...), actions, plot
state (crying, torn pantyhose arcs), scene tokens, weight syntax, punctuation.

Clothing chains: positive blocks mean "worn", negative blocks (-N::) mean
"removed + anti re-wear". Replace the token on BOTH sides of the chain, keep
the weights and grid positions identical.
