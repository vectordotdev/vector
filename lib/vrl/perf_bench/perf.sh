#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd $(dirname "$BASH_SOURCE[0]") && pwd)" &> /dev/null
MANIFEST_PATH="$SCRIPT_DIR/Cargo.toml"
EXECUTABLE_PATH="$SCRIPT_DIR/../../../target/release/perf_bench"

DATE=$(date '+%Y-%m-%d-%H-%M-%S')
PERF_COMMAND="$1"

if [ -z "$PERF_COMMAND" ]; then
	cargo build --manifest-path="$MANIFEST_PATH" --profile release
else
	cargo build --manifest-path="$MANIFEST_PATH" --profile bench
fi

OUTPUTS_DIR="$SCRIPT_DIR/outputs/$DATE"

mkdir -p "$OUTPUTS_DIR"

for FILE in "$SCRIPT_DIR/inputs/"*.vrl; do
    PROGRAM=$(cat $FILE | sed 's/\\/\\\\/g' | sed 's/"/\\"/g' | sed 's/\[/\\\[/g' | sed 's/\]/\\\]/g')
    FILENAME="$(basename "$FILE" | cut -d. -f1)"
    OPTIMIZATION_LEVEL="aggressive"

    for TARGET_FILE in "$SCRIPT_DIR/inputs/$FILENAME."*.json; do
        TARGET=$(cat $TARGET_FILE | sed 's/\\/\\\\/g' | sed 's/"/\\"/g' | sed 's/\[/\\\[/g' | sed 's/\]/\\\]/g')
        TARGET_FILENAME="$(basename "$TARGET_FILE" | cut -d. -f1)"

        for RUNTIME in "ast" "vectorized" "llvm"; do
			COMMANDS=$(cat <<-END
				set timeout -1
				spawn $EXECUTABLE_PATH
				set vrl_id \$spawn_id
				set vrl_pid [exp_pid]
				expect "program: "
				send "$PROGRAM\n\004"
				expect "runtime: "
				send "$RUNTIME\n"
				set timeout 1
				expect "optimization_level" {
					send "$OPTIMIZATION_LEVEL\n"
				}
				set timeout -1
			END
            )

            for BATCH_SIZE in "1" "10" "20" "30" "40" "50" "60" "70" "80" "90" "100" "200" "300" "400" "500" "600" "700" "800" "900" "1000" "2000" "3000" "4000"; do
                NUM_ITERATIONS="1000"
                NUM_ITERATIONS_WARMUP="$NUM_ITERATIONS"

                FILE_RESULT="$OUTPUTS_DIR/$TARGET_FILENAME-$RUNTIME-$BATCH_SIZE-$NUM_ITERATIONS-$NUM_ITERATIONS_WARMUP.json"

                COMMANDS+=$(cat <<-END

					expect -i \$vrl_id "target: "
					send -i \$vrl_id "$TARGET\n\004"
					expect -i \$vrl_id "num_iterations_warmup: "
					send -i \$vrl_id "$NUM_ITERATIONS_WARMUP\n"
					expect -i \$vrl_id "num_iterations: "
					send -i \$vrl_id "$NUM_ITERATIONS\n"
					expect -i \$vrl_id "batch_size: "
					send -i \$vrl_id "$BATCH_SIZE\n"
					expect -i \$vrl_id "Warming up..."
					expect -i \$vrl_id "Press enter to begin."
				END
                )

                if [ ! -z "$PERF_COMMAND" ]; then
					COMMANDS+=$(cat <<-END

						spawn $PERF_COMMAND --pid=\$vrl_pid
						set perf_id \$spawn_id
						set perf_pid [exp_pid]
					END
                    )
                fi

                COMMANDS+=$(cat <<-END

					send -i \$vrl_id "\n"
					expect -i \$vrl_id "Write results to path (enter for stdout): "
				END
                )


                if [ ! -z "$PERF_COMMAND" ]; then
                    COMMANDS+=$(cat <<-END

						send -i \$perf_id "\x03"
						expect -i \$perf_id *
					END
                    )
                fi

                COMMANDS+=$(cat <<-END

					send -i \$vrl_id "$FILE_RESULT\n"
				END
                )
            done

            COMMANDS+=$(cat <<-END

				expect -i \$vrl_id "target: "
				send -i \$vrl_id "\x03"
				interact -i \$vrl_id
			END
            )

            echo "$COMMANDS"

            expect -c "$COMMANDS"
        done
    done
done
