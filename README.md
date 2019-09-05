# Epic Cash

Epic Cash is an in-progress implementation of the MimbleWimble protocol. Epic Cash redefined new core characteristics of this new privacy focused blockchain forked from Grin, following constitutes a first set of choices:

  * Block rewards epoch & total supply to match Bitcoin
  * Support diversity of mining hash powers using a combination of algorithms
  * Clean and minimal implementation, and aiming to stay as such.
  * Follows the MimbleWimble protocol, which provides great anonymity and scaling characteristics.
  * Cuckoo Cycle proof of work with the CuckAToo31+ (ASIC-targeted).
  * Relatively fast block time: one minute.
  * Transaction fees are based on the number of Outputs created/destroyed and total transaction size.
  * Smooth curve for difficulty adjustments.

## Status

Epic is live with mainnet.

## Getting Started

To build and try out Epic, see the [build docs](doc/build.md).

To run the Epic Cash blockchain on Linux distributions, see the tutorial of [How to run the Epic Cash blockchain on Linux](doc/running.org).

To run the Epic Cash blockchain on windows, see the tutorial of [How to run the Epic Cash blockchain on Windows](doc/windows.org).

## Credits

Tom Elvis Jedusor for the first formulation of MimbleWimble.

Andrew Poelstra for his related work and improvements.

John Tromp for the Cuckoo Cycle proof of work.

Grin Developers for the initial implementation

J.K. Rowling for making it despite extraordinary adversity.

## License

Apache License v2.0.
