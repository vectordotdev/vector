A new configuration option `rotate_wait_secs` was added to the `file` and `kubernetes_logs` sources. `rotate_wait_secs` determines for how long Vector keeps trying to read from a log file that has been deleted. Once that time span has expired, Vector stops reading from and closes the file descriptor of the deleted file, thus allowing the OS to reclaim the storage space occupied by the file.

authors: syedriko
