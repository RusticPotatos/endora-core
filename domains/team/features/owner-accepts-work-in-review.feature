# ADR 0149 (owner acceptance of in-review work). A task reaches `done` because its criteria were
# satisfied AND an authorized acceptance decision occurred. A loop one member works alone has no
# reviewer, and HQ never invents one: under the default policy `owner`, setting the item ready
# puts it on the owner's desk (the existing "ready to read" card) to accept or send back. Under
# the owner's explicit opt-out `unverified` it closes itself as completed, not verified, and is
# never shown or counted as accepted. Four outcomes stay apart: completed, accepted, rejected
# (sent back) and abandoned (dropped).
Feature: The owner accepts work that nobody reviews

  Background:
    Given the team is one team of models
      | member | model  |
      | Mara   | opus   |
      | Kit    | sonnet |
      | Ada    | haiku  |
      | Sol    | haiku  |
    And the loop "ansible-nas" for the goal "Keep the fork tidy"
    And the loop "solo arm"
    And the owner sets the loop "solo arm" to be worked by Sol alone

  Scenario: Completed work is not accepted work: the default policy is owner, and nothing closes it
    Given the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    Then the loop "solo arm" accepts work as "owner"
    And the task "Settle the VPN restart" is in_review
    And "Settle the VPN restart" has no closing on record
    And the owner's desk holds one card to accept "Settle the VPN restart"
    And no member is asked to review "Settle the VPN restart"

  Scenario: The owner accepts it: done, closed as accepted by the owner, who and when recorded
    Given the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    When the owner accepts "Settle the VPN restart"
    Then the task "Settle the VPN restart" is done
    And "Settle the VPN restart" closed as "accepted_by_owner": "the owner accepted it"
    And "Settle the VPN restart" was closed by "owner"
    And the feed says "Settle the VPN restart" is done, verified
    And the owner's desk holds nothing about "Settle the VPN restart"

  Scenario: At the engine a second accept of the same card is refused and changes nothing
    Given the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    And the owner accepts "Settle the VPN restart"
    When the owner accepts "Settle the VPN restart"
    Then it is refused for "this task is over"
    And the task "Settle the VPN restart" is done

  Scenario: A stale card is refused, with no state change
    Given the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    And "Settle the VPN restart" is put back to doing behind the card's back
    When the owner accepts "Settle the VPN restart"
    Then it is refused for "a task does not go from doing to done"
    And the task "Settle the VPN restart" is doing
    And "Settle the VPN restart" has no closing on record
    And the owner's desk holds one card to accept "Settle the VPN restart"
    When the owner sends "Settle the VPN restart" back
    Then it is refused for "a task does not go from doing to doing"
    And the task "Settle the VPN restart" is doing

  Scenario: Gated work is accepted only while its gate has passed
    Given the loop "solo arm" has the item "Document the media role": "Write roles/media/README.md from the role's defaults."
    And a tick passes
    When Sol takes "Document the media role" and reads media's logs: "media: listening on 8096, ready"
    And Sol submits the branch "hq/document-the-media-role", changing "roles/media/README.md"
    And the check passes: "syntax ok, ansible-lint ok, yamllint ok"
    And Sol asks for review of "Document the media role", citing her reading
    Then the task "Document the media role" is in_review
    When the gate of "Document the media role" fails afterwards: "yamllint failed"
    And the owner accepts "Document the media role"
    Then it is refused for "the gate has not passed"
    And the task "Document the media role" is in_review
    When the check passes again for "Document the media role"
    And the owner accepts "Document the media role"
    Then the task "Document the media role" is done
    And "Document the media role" closed as "accepted_by_owner": "the owner accepted it"

  Scenario: Rejected work goes back to its worker with the owner's note, and can be accepted after a second review
    Given the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    When the owner sends "Settle the VPN restart" back, saying "Check the second restart too."
    Then the task "Settle the VPN restart" is doing
    And "Settle the VPN restart" has no closing on record
    And the thread of "Settle the VPN restart" says "Check the second restart too."
    And Sol is woken by a send-back on "Settle the VPN restart"
    And the owner's desk holds nothing about "Settle the VPN restart"
    When Sol reads gluetun's logs again: "gluetun: healthy after the second restart"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    Then the task "Settle the VPN restart" is in_review
    And the owner's desk holds one card to accept "Settle the VPN restart"
    When the owner accepts "Settle the VPN restart"
    Then "Settle the VPN restart" closed as "accepted_by_owner": "the owner accepted it"

  Scenario: Rejecting work in a paused project is refused until the project is resumed, then it goes back as usual
    Given the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    And the owner pauses the loop "solo arm"
    When the owner sends "Settle the VPN restart" back
    Then it is refused for "project is paused"
    And it is refused for "resume"
    And the task "Settle the VPN restart" is in_review
    And "Settle the VPN restart" has no closing on record
    And the thread of "Settle the VPN restart" holds no word from the owner
    And Sol is not woken by a send-back on "Settle the VPN restart"
    And the owner's desk holds one card to accept "Settle the VPN restart"
    When the owner resumes the loop "solo arm"
    And the owner sends "Settle the VPN restart" back, saying "Check the second restart too."
    Then the task "Settle the VPN restart" is doing
    And the thread of "Settle the VPN restart" says "Check the second restart too."
    And Sol is woken by a send-back on "Settle the VPN restart"
    And the owner's desk holds nothing about "Settle the VPN restart"
    When Sol reads gluetun's logs again: "gluetun: healthy after the second restart"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    Then the task "Settle the VPN restart" is in_review
    And the owner's desk holds one card to accept "Settle the VPN restart"
    When the owner accepts "Settle the VPN restart"
    Then "Settle the VPN restart" closed as "accepted_by_owner": "the owner accepted it"

  Scenario: Accepting work in a paused project still closes it, accepted by the owner
    Given the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    And the owner pauses the loop "solo arm"
    When the owner accepts "Settle the VPN restart"
    Then the task "Settle the VPN restart" is done
    And "Settle the VPN restart" closed as "accepted_by_owner": "the owner accepted it"
    And "Settle the VPN restart" was closed by "owner"

  Scenario: Abandoned work stays dropped, with the reason it was given
    Given the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    When the owner drops "Settle the VPN restart" as "wont_do"
    Then the task "Settle the VPN restart" is dropped
    And "Settle the VPN restart" closed as "wont_do": "it will not be done"

  Scenario: Cleaning up stranded work never closes it: the card is not duplicated and nothing is done or dropped
    Given the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    When 30 hours pass, with HQ ticking every few minutes
    Then the task "Settle the VPN restart" is in_review
    And "Settle the VPN restart" has no closing on record
    And the owner's desk holds one card to accept "Settle the VPN restart"

  Scenario: Under the owner's opt-out, completed work is closed as completed, not verified, and is never accepted
    Given the owner lets the loop "solo arm" close its work unverified
    And the loop "solo arm" has the item "Settle the VPN restart": "Is the 2-hourly restart still needed?"
    And a tick passes
    When Sol takes "Settle the VPN restart" and reads gluetun's logs: "gluetun: healthy"
    And Sol asks for review of "Settle the VPN restart", citing her reading
    Then the loop "solo arm" accepts work as "unverified"
    And the task "Settle the VPN restart" is done
    And "Settle the VPN restart" closed as "completed_unverified": "it was completed, not verified: its evidence was cited, and nothing checked that it settles the item"
    And the feed says "Settle the VPN restart" is done, not verified
    And the owner's desk holds nothing about "Settle the VPN restart"
