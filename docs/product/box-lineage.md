# Box lineage inspector

Box details expose an optional inspector for the immediate producing and spending
transactions. Neither transaction is requested until its Inspect button is activated.
Each side makes at most one successful detail request, with explicit retries for errors.
There is no recursive graph fetch or automatic expanded-transaction walk.

The producing side lists previous input boxes and sibling outputs; the spending side
lists following outputs and offers a disclosure for its other inputs. Links open the
ordinary box, token, block and transaction pages. Values retain integer precision.
Connections describe transaction membership, not owners, payment paths or an allocation
of input value to particular outputs.

The inspector validates transaction ID, selected-box membership, output positions and
the recorded spending height before displaying fetched details. Inclusion height comes
from the transaction, never from the box's declared creation height. Each transaction
reports its own indexed detail snapshot; the two responses are not a shared snapshot.
Unknown input boxes retain their IDs and explicitly lack values. No input total is
computed. Genesis's zero producing ID and an unspent box record have explicit terminal
states. Refresh box details reloads the page data to recheck spend references.

The helper rejects more than 10,000 combined input/output references. Each list initially
shows four boxes and expands to at most 20, with counts and full transaction links for
the rest. Each box shows at most two token amounts; remaining tokens are disclosed as a
count. API detail-budget failures, unavailable transactions and mismatched membership
remain distinct, without fabricated graph edges. Stopped controllers discard every late
result or error after navigation or unmount.

Default uses a left-to-right lineage, Prism places the selected box above two spatial
branches, Atelier uses an editorial record margin and ledger rows, and Aurora uses a
large selected-box column beside stacked transaction chapters. Mobile layouts retain
the producing/current/spending reading order and native keyboard-operable controls.
