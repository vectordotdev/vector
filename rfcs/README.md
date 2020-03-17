# RFCs

Vector uses the RFC process to formalize discussion around _substantial_
changes to Vector. The general goals are:

* Properly spec and plan features to prevent re-work
* Formalize discussion
* Obtain consensus
* Share reponsibility
* Record the decision for posterity

## Guidelines

### Logical boundary

Examples of changes that require a RFC:

* An architectural change.
* A data model change.
* A new component that introduces new behavior.
* Removing a feature.
* Complicated tech-debt projects.
* A substantial user-visible change.
* A change that is questionably outside of the scope of Vector.

Examples of changes that do not require a RFC:

* Reorganizing code that otherwise does not change its functional behavior.
* Quantitative improvements. Such as performance improvements.
* Simple improvements to existing features.

### Before creating a RFC

1. Search Github for previous issues and RFCs on this topic.
2. Open an issue representing the change for light discussion.
3. In the isuee, obtain consensus that a RFC is necessary.
   * The change might get quickly rejected.
   * The change might be on our long term roadmap and get deferred.
   * The change might be blocked by other work.

### Creating a RFC

1. Create a new branch with the `rfcs/YYYY-MM-DD-issue#-title.md` file.
   * Example: `rfcs/2020-02-10-445-internal-observability.md`
2. Submit your RFC as a pull request for discussion.
3. At least 3 other team members must approve your RFC.
4. Create issues representing the individual changes.
5. Virtual high-five your team members and begin work.
