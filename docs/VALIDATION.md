# Validation and findings

Inspection records credential presence (`present`, `absent`, `unreadable`), integrity (`valid`, `invalid`, `unknown`), and trust (`trusted`, `untrusted`, `not_evaluated`, `unknown`) independently. Detailed SDK JSON is included in each inspection. `parent_link` is one of `linked`, `wrong_parent`, `missing_parent`, `unverifiable`, or `preserved` (passthrough).

Stable finding codes currently emitted:

| Code                            | Meaning                                                                                                                 |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `UNSIGNED_SOURCE`               | Input has no readable embedded credentials; origin remains unknown.                                                     |
| `CREDENTIALS_MISSING`           | Output does not contain readable credentials. In raw compare branch this is marked as an expected negative observation. |
| `CONTENT_BINDING_INVALID`       | SDK reports invalid output credentials.                                                                                 |
| `SIGNATURE_UNTRUSTED`           | SDK reports valid integrity but trust is not established.                                                               |
| `PARENT_LINK_MISSING`           | Reissued output does not evidence the expected immediate parent.                                                        |
| `PARENT_LINK_MISMATCH`          | A parent ingredient exists but does not identify the expected parent manifest.                                          |
| `CHAIN_COMPLETE`                | Child ingredient references the expected parent's active manifest.                                                      |
| `TRANSFORMATION_ACTION_MISSING` | Reissued manifest omits the action expected for the recorded operation.                                                 |
| `EVIDENCE_INSUFFICIENT`         | Credential data could not be parsed; absence is not established.                                                        |
| `POLICY_VIOLATION`              | A selected policy requirement failed.                                                                                   |

Trust evaluation depends on the SDK context and configured trust material; test identity output is expected to be untrusted. An `unknown` state is not a pass.
