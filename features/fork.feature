Feature: check fork features

Scenario: original time tolerance rule
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <progpow> equals <100>
  And I have the policy <0> with <cuckatoo> equals <0>
  And I setup all the policies
  Given I have a <floonet> chain
  And I setup a chain with genesis block mined with <progpow>
  Given I define my output dir as <.epic.fork1>
  Given I create a block <progpow> with future timespan <720>

Scenario: change in the time tolerance rule
  Given I have the policy <0> with <cuckaroo> equals <0>
  And I have the policy <0> with <progpow> equals <100>
  And I have the policy <0> with <cuckatoo> equals <0>
  And I setup all the policies
  Given I have a <floonet> chain
  And I setup a chain with genesis block mined with <progpow>
  Given I define my output dir as <.epic.fork1>
  Given I create a block <progpow> with future timespan <0>
  Given I create a block <progpow> with future timespan <10>
  Given I create a block <progpow> with future timespan <30>
