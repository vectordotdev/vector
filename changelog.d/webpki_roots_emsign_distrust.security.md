Updated the bundled Mozilla root certificate store to remove the `emSign ECC Root CA - G3` root following the eMudhra intermediate certificate compromise, along with other roots Mozilla has since distrusted. TLS connections to servers whose chains end at a removed root now fail validation.

authors: thomasqueirozb
