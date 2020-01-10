#!/bin/sh

dpkg-reconfigure qemu-user-binfmt # register qemu in the kernel
exec $@
