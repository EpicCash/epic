# Epic Cash

Epic Cash is an in-progress implementation of the MimbleWimble protocol. Epic Cash redefined new core characteristics of this new privacy focused blockchain forked from Grin, following constitutes a first set of choices:

  * Block rewards epoch & total supply to match Bitcoin
  * Support diversity of mining hash powers using a combination of algorithms
  * Clean and minimal implementation, and aiming to stay as such.
  * Follows the MimbleWimble protocol, which provides great anonymity and scaling characteristics.
  * Cuckoo Cycle proof of work in two variants named Cuckaroo (ASIC-resistant) and Cuckatoo (ASIC-targeted).
  * Relatively fast block time: one minute.
  * Transaction fees are based on the number of Outputs created/destroyed and total transaction size.
  * Smooth curve for difficulty adjustments.

To learn more, read our [introduction to MimbleWimble and Epic](doc/intro.md).

## Status

Epic is live with testnet. 
## Getting Started

To learn more about the technology, read our [introduction](doc/intro.md).

To build and try out Epic, see the [build docs](doc/build.md).

To run the Testnet, see the tutorial of [how to run the test net](doc/running.org).

## Credits

Tom Elvis Jedusor for the first formulation of MimbleWimble.

Andrew Poelstra for his related work and improvements.

John Tromp for the Cuckoo Cycle proof of work.

Grin Developers for the initial implementation

J.K. Rowling for making it despite extraordinary adversity.

## License

Apache License v2.0.
