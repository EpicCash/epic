Feature: Mine a simple chain
 
Scenario: test the creation of buffers for multi algo adjustment
  Given I have the policy <0> with <progpow> equals <38>
  And I have the policy <0> with <randomx> equals <60>
  And I have the policy <0> with <cuckatoo> equals <2>
  And I setup all the policies
  Given I have a <testing> chain
  And I create the genesis block with initial timestamp of <1566241802> and mined with <cuckatoo>
  And I create a chain and add the genesis block
  And I define my output dir as <.epicdifficulty>
  Then I create a buffer of <10> <cuckatoo> that I had to complete <9> blocks
  Then I add <1> blocks with increasing timestamp following the policy <0>
  Then I create a buffer of <10> <randomx> that I had to complete <9> blocks
  Then I add <1> blocks with increasing timestamp following the policy <0>
  Then I create a buffer of <10> <progpow> that I had to complete <9> blocks
  Then I add <4> blocks with increasing timestamp following the policy <0>
  And I create a buffer of <10> <randomx> that I had to complete <6> blocks
  Then I add <1> blocks with increasing timestamp following the policy <0>
  And I create a buffer of <10> <progpow> that I had to complete <7> blocks
  Then I add <1> blocks with increasing timestamp following the policy <0>
  And I create a buffer of <10> <randomx> that I had to complete <5> blocks
  Then I add <1> blocks with increasing timestamp following the policy <0>
  And I create a buffer of <10> <progpow> that I had to complete <6> blocks
  Then I add <2> blocks with increasing timestamp following the policy <0>
  And I create a buffer of <10> <randomx> that I had to complete <3> blocks
  Then I add <1> blocks with increasing timestamp following the policy <0>
  And I create a buffer of <10> <progpow> that I had to complete <5> blocks
  Then I add <2> blocks with increasing timestamp following the policy <0>
  And I create a buffer of <10> <randomx> that I had to complete <1> blocks
  And I create a buffer of <9> <randomx> that I had to complete <0> blocks
  And I create a buffer of <8> <randomx> that I had to complete <0> blocks
  Then I add <1> blocks with increasing timestamp following the policy <0>
  And I create a buffer of <10> <progpow> that I had to complete <4> blocks
  And I create a buffer of <6> <progpow> that I had to complete <0> blocks
  And I create a buffer of <5> <progpow> that I had to complete <0> blocks
  



Scenario: test the multi difficulty adjustment with custom timestamps
  Given I have the policy <0> with <progpow> equals <38>
  Given I have the policy <0> with <randomx> equals <60>
  Given I have the policy <0> with <cuckatoo> equals <2>
  Given I setup all the policies
  Given I have a <testing> chain
  And I create the genesis block with initial timestamp of <1566241802> and mined with <cuckatoo>
  And I create a chain and add the genesis block
  And I define my output dir as <.epicdifficulty2>
  Given I create a block <randomx> with timespan <20>
  Given I create a block <progpow> with timespan <60>
  Given I create a block <randomx> with timespan <5>
  Given I create a block <progpow> with timespan <120>
  Given I create a block <randomx> with timespan <3>
  Given I create a block <randomx> with timespan <30>
  Given I create a block <progpow> with timespan <70>
  Given I create a block <randomx> with timespan <29>
  Given I create a block <progpow> with timespan <90>
  Given I create a block <randomx> with timespan <34>
  Given I create a block <randomx> with timespan <70>
  Given I create a block <progpow> with timespan <15>
  Then The block on the height <0> need have a time delta of <60>
  And The block on the height <1> need have a time delta of <60>
  And The block on the height <2> need have a time delta of <60>
  And The block on the height <3> need have a time delta of <5>
  And The block on the height <4> need have a time delta of <120>
  And The block on the height <5> need have a time delta of <3>
  And The block on the height <6> need have a time delta of <30>
  And The block on the height <7> need have a time delta of <70>
  And The block on the height <8> need have a time delta of <29>
  And The block on the height <9> need have a time delta of <90>
  And The block on the height <10> need have a time delta of <34>
  And The block on the height <11> need have a time delta of <70>
  And The block on the height <12> need have a time delta of <15>
  Then The next_difficulty for block <2> need to be <8064>
  And The next_difficulty for block <3> need to be <132120576>
  And The next_difficulty for block <4> need to be <8190>
  And The next_difficulty for block <5> need to be <125927424>
  And The next_difficulty for block <6> need to be <8317>
  And The next_difficulty for block <7> need to be <8446>
  And The next_difficulty for block <8> need to be <123959808>
  And The next_difficulty for block <9> need to be <8577>
  And The next_difficulty for block <10> need to be <122022936>
  And The next_difficulty for block <11> need to be <8711>
  And The next_difficulty for block <12> need to be <8575>
  And The next_difficulty for block <13> need to be <123929544>

Scenario: match the mining and foundation rewards with the whitepaper
  Given I have a <mainnet> chain
  And All rewards match the whitepaper
  Then I test if the cumulative foundation levy is being computed correctly

Scenario: add hardcoded coinbase
  Given I have a hardcoded coinbase

Scenario: add coinbase to each mined block
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic-coinbase>
  And I add foundation wallet pubkeys
  And I add a genesis block with coinbase and mined with <cuckatoo>
  And I setup the chain for coinbase test
  Then I add <4> blocks following the policy <0>
  Then I add block with foundation reward following the policy <0>

Scenario: refuse a foundation output invalid
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic-coinbase>
  And I add foundation wallet pubkeys
  And I add a genesis block with coinbase and mined with <cuckatoo>
  And I setup the chain for coinbase test
  Then I add <4> blocks following the policy <0>
  Then Refuse a foundation commit invalid

Scenario: check a policy sequence of cuckatoo using feijoada deterministic
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Then Check the next algorithm <cuckatoo>
  Then Increase bottles <cuckatoo>
  Then Check the next algorithm <cuckatoo>
  Then Increase bottles <cuckatoo>
  Then Check the next algorithm <cuckatoo>

Scenario: check a policy sequence of cuckatoo and randomx using feijoada deterministic
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <50>
  And I have the policy <0> with <cuckatoo> equals <50>
  And I setup all the policies
  Then Check the next algorithm <randomx>
  Then Increase bottles <randomx>
  Then Check the next algorithm <cuckatoo>
  Then Increase bottles <cuckatoo>
  Then Check the next algorithm <randomx>
  Then Increase bottles <randomx>
  Then Check the next algorithm <cuckatoo>

Scenario: check if blocks added in a blockchain match the policy
  Given I have the policy <0> with <cuckaroo> equals <33>
  And I have the policy <0> with <randomx> equals <33>
  And I have the policy <0> with <cuckatoo> equals <34>
  And I setup all the policies
  Given I have a <testing> chain
  And I setup a chain with genesis block mined with <randomx>
  And I define my output dir as <.epicpolicy>
  Then I add <99> blocks following the policy <0>
  Then I check if the bottle matches the policy
  Then I add <1> blocks following the policy <0>
  Then I check if the bottle is being emptied
  Then I add <99> blocks following the policy <0>
  Then I check if the bottle matches the policy

Scenario: check if accept multi policies
  Given I set default policy config
  Given I have a <testing> chain
  Given I set the allowed policy on the height <5> with value <3>
  And I setup a chain with genesis block mined with <randomx>
  And I define my output dir as <.epicpolicy>
  Then I add <5> blocks following the policy <0>
  Then I add <3> blocks following the policy <1>
  Then I add <2> blocks following the policy <0>
  Then I add <3> blocks following the policy <1>

Scenario: refuse blocks that were not mined with a desired algorithm
  Given I have the policy <0> with <cuckaroo> equals <33>
  And I have the policy <0> with <randomx> equals <33>
  And I have the policy <0> with <cuckatoo> equals <34>
  And I setup all the policies
  Given I have a <testing> chain
  And I setup a chain with genesis block mined with <randomx>
  And I define my output dir as <.epicpolicy>
  Then I add <5> blocks mined with <randomx> and accept <0>
  Then I add <5> blocks mined with <cuckatoo> and accept <1>
  Then I add <5> blocks mined with <randomx> and accept <0>
  Then I add <5> blocks mined with <cuckaroo> and accept <1>
  Then I add <5> blocks mined with <cuckatoo> and accept <1>
  Then I add <5> blocks mined with <randomx> and accept <1>

Scenario: mine empty chain
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic>
  Then mine an empty keychain
  Then clean output dir

Scenario: mine genesis reward chain
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic.genesis>
  And I add coinbase data from the dev genesis block
  Then I get a valid PoW
  Then I mine
  Then clean tmp chain dir
  Then clean output dir

Scenario: mine cuckatoo genesis reward chain
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  Given I define my output dir as <.epic.genesis>
  Given I add coinbase data from the dev genesis block
  Then I get a valid <cuckatoo> PoW
  Then I mine
  Then clean tmp chain dir
  Then clean output dir

Scenario: mine forks
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic2>
  And I setup a chain
  And I make <1> blocks
  Then I mine and add a few blocks
  Then clean output dir

Scenario: mine losing forks
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic3>
  And I setup a chain
  And I make <2> blocks
  Then I fork and mine in the chain lost

Scenario: longer fork
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic4>
  And I setup a chain
  And I make <10> blocks
  Then I make <7> blocks forked in the height <5>
  Then the chain need to be on the height <12>

Scenario: spend in fork and compact
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic5>
  And I setup a chain
  Then I spend in different forks

Scenario: output header mappings
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <0>
  And I have the policy <0> with <cuckatoo> equals <100>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic_header_for_output>
  And I setup a chain
  Then I check outputs in the header

# md5 tests
# Scenario: mine md5 genesis reward chain
#  Given I have the policy <0> with <cuckaroo> equals <100>
#  And I have the policy <0> with <randomx> equals <0>
#  And I have the policy <0> with <cuckatoo> equals <0>
#  And I setup all the policies
#  Given I have a <testing> chain
#  Given I setup a chain
#  Given I define my output dir as <.epic.genesis>
#  Given I add coinbase data from the dev genesis block
#  Then I get a valid <md5> PoW
#  Then I mine <md5>
#  Then clean tmp chain dir
#  Then clean output dir

#Scenario: accept valid md5
#  Given I have the policy <0> with <cuckaroo> equals <100>
#  And I have the policy <0> with <randomx> equals <0>
#  And I have the policy <0> with <cuckatoo> equals <0>
#  And I setup all the policies
#  Given I have a <testing> chain
#  And I define my output dir as <.epic11>
#  And I setup a chain
#  Then I accept a block with a pow <md5> valid
#  Then clean tmp chain dir
#  Then clean output dir

#Scenario: refuse invalid md5 pow
#  Given I have the policy <0> with <cuckaroo> equals <100>
#  And I have the policy <0> with <randomx> equals <0>
#  And I have the policy <0> with <cuckatoo> equals <0>
#  And I setup all the policies
#  Given I have a <testing> chain
#  And I define my output dir as <.epic6>
#  And I setup a chain
#  Then I refuse a block with <md5> invalid

# randomx tests
Scenario: mine randomx genesis reward chain
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <100>
  And I have the policy <0> with <cuckatoo> equals <0>
  And I setup all the policies
  Given I have a <testing> chain
  Given I setup a chain
  Given I define my output dir as <.epic.genesis20>
  Given I add coinbase data from the dev genesis block
  Then I get a valid <randomx> PoW
  Then I mine <randomx>
  Then clean tmp chain dir
  Then clean output dir

Scenario: accept valid randomx
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <100>
  And I have the policy <0> with <cuckatoo> equals <0>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic11>
  And I setup a chain
  Then I accept a block with a pow <randomx> valid
  Then clean tmp chain dir
  Then clean output dir

Scenario: refuse invalid randomx pow
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <randomx> equals <100>
  And I have the policy <0> with <cuckatoo> equals <0>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic10>
  And I setup a chain
  Then I refuse a block with <randomx> invalid
  Then clean tmp chain dir
  Then clean output dir

# progpow tests
Scenario: mine progpow genesis reward chain
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <progpow> equals <100>
  And I have the policy <0> with <cuckatoo> equals <0>
  And I setup all the policies
  Given I have a <testing> chain
  Given I setup a chain
  Given I define my output dir as <.epic.genesis20>
  Given I add coinbase data from the dev genesis block
  Then I get a valid <progpow> PoW
  Then I mine <progpow>
  Then clean tmp chain dir
  Then clean output dir

Scenario: accept valid progpow
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <progpow> equals <100>
  And I have the policy <0> with <cuckatoo> equals <0>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic11>
  And I setup a chain
  Then I accept a block with a pow <progpow> valid
  Then clean tmp chain dir
  Then clean output dir

Scenario: refuse invalid progpow pow
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <progpow> equals <100>
  And I have the policy <0> with <cuckatoo> equals <0>
  And I setup all the policies
  Given I have a <testing> chain
  And I define my output dir as <.epic10>
  And I setup a chain
  Then I refuse a block with <progpow> invalid
  Then clean tmp chain dir
  Then clean output dir
