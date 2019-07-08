# Contributing

First, thank you for contributing to Vector! Our goal is to make it as easy
as possible to contribute while still protecting users of Vector.

1. Your commits must include a [DCO](https://developercertificate.org/) signature.
   This is simpler than it sounds; it just means that all of your commits
   must contain:

   ```
   Signed-off-by: Joe Smith <joe.smith@email.com>
   ```

   Git makes this easy by adding the `-s` or `--signoff` flags when you commit:

   ```bash
   git commit -sm 'My commit message'
   ```

2. Open a pull request.
3. At least one Vector team member must approve your work before merging.

## FAQ

### What is a DCO?

DCO stands for Developer Certificate of Origin and is maintained by the
[Linux Foundation](https://www.linuxfoundation.org). It is an attestation
attached to every commit made by every developer. It ensures that all commited
code adheres to the [Vector license](LICENSE.md) (Apache 2.0).

### Why does Vector adopt the DCO?

To protect the users of Vector. It ensures that all Vector contributors, and
committed code, agree to the [Vector license](LICENSE.md).

### Why a DCO instead of a CLA?

It's simpler, clearer, and still protects users of Vector. We believe the DCO
more accurately embodies the principles of open-source. More info can be found
here:

* [Gitlab's switch to DCO](https://about.gitlab.com/2017/11/01/gitlab-switches-to-dco-license/)
* [DCO vs CLA](https://opensource.com/article/18/3/cla-vs-dco-whats-difference)

### What about trivial changes?

Trivial changes, such as spelling fixes, do not need to be signed.

### Granted rights and copyright assignment

It is important to note that the DCO is not a license. The license of the
project – in our case the Apache License – is the license under which the
contribution is made. However, the DCO in conjunction with the Apache License
may be considered an alternate CLA.

The existence of section 5 of the Apache License is proof that the Apache
License is intended to be usable without CLAs. Users need for the code to be
open source, with all the legal rights that implies, but it is the open source
license that provides this. The Apache License provides very generous
copyright permissions from contributors, and contributors explicitly grant
patent licenses as well. These rights are granted to everyone.

### If I’m contributing while an employee, do I still need my employer to sign something?

Nope! The DCO confirms that you are entitled to submit the code, which assumes
that you are authorized to do so.  It treats you like an adult and relies on
your accurate statement about your rights to submit a contribution.  

### What if I forgot to sign my commits?

No probs! We made this simple with the [`signoff` Makefile target](Makefile):

```bash
make signoff
```

If you prefer to do this manually:

https://stackoverflow.com/questions/13043357/git-sign-off-previous-commits

## Credits

*Many thanks to the [Apache](http://www.apache.org/) and
[Linux](https://www.linuxfoundation.org/) software foundatons for providing
the framework and inspiration for these agreements.*

