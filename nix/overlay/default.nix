self: super:

{
  # This exists for Centos7 compat reasons.
  vector = {
    development = {
      # glibc_2_17 = (super.callPackage 
      #   (import ./glibc/2.17/default.nix)
      #   {
      #     inherit (self);
      #   }
      # );
    };
  };
}
