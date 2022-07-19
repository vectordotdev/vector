cargo build --profile bench

for file in ./inputs/*; do
    program=$(cat $file | sed 's/\\/\\\\/g' | sed 's/"/\\"/g')
    target="{}"
    num_iterations_warmup="1000"
    num_iterations="1000"
    batch_size="1000"

    for runtime in "ast" "ast_batched"; do
        commands=$(cat <<-END
			set timeout -1
			spawn ../../../target/release/perf_vrl
			set vrl_id \$spawn_id
			set vrl_pid [exp_pid]
			expect "program: "
			send "$program\n\004"
			expect "target: "
			send "$target\n"
			expect "num_iterations_warmup: "
			send "$num_iterations_warmup\n"
			expect "num_iterations: "
			send "$num_iterations\n"
			expect "batch_size: "
			send "$batch_size\n"
			expect "runtime: "
			send "$runtime\n"
			expect -i \$vrl_id "Press enter to begin."
			spawn perf record --pid=\$vrl_pid
			set perf_id \$spawn_id
			set perf_pid [exp_pid]
			send -i \$vrl_id "\n"
			expect -i \$vrl_id "Press enter to end."
			send -i \$perf_id "\x03"
			expect -i \$perf_id *
			send -i \$vrl_id "\n"
			interact -i \$vrl_id
		END
        )

        expect -c "$commands"
    done
done
