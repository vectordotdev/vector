use std::cmp::min;

fn min3<T>(a: T, b: T, c: T) -> T
where
    T: Ord,
{
    min(a, min(b, c))
}

// Calculates the damerau-levenstein distance - the number of edits needed to
// change one word into another, taking into account transposed letters.
pub(crate) fn distance(word1: &[char], word2: &[char]) -> usize {
    let m = word1.len() + 1;
    let n = word2.len() + 1;

    // Setup a matrix between the two strings
    let mut matrix = (0..m * n).map(|_| 0_usize).collect::<Vec<_>>();

    // Make it easier to get the correct index in the matrix
    let pos = |a, b| b * m + a;

    for col in 1..m {
        matrix[pos(col, 0)] = col;
    }

    for row in 1..n {
        matrix[pos(0, row)] = row;
    }

    for row in 1..n {
        for col in 1..m {
            let cost = usize::from(word1[col - 1] != word2[row - 1]);

            matrix[pos(col, row)] = min3(
                matrix[pos(col - 1, row)] + 1,
                matrix[pos(col, row - 1)] + 1,
                matrix[pos(col - 1, row - 1)] + cost,
            );

            // Check for transposed letters.
            if row > 1
                && col > 1
                && word1[col - 1] == word2[row - 2]
                && word1[col - 2] == word2[row - 1]
            {
                matrix[pos(col, row)] =
                    min(matrix[pos(col, row)], matrix[pos(col - 2, row - 2)] + 1)
            }
        }
    }

    matrix[matrix.len() - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenstein() {
        assert_eq!(
            1,
            distance(
                &"cat".chars().collect::<Vec<_>>(),
                &"cot".chars().collect::<Vec<_>>()
            )
        );

        assert_eq!(
            1,
            distance(
                &"ct".chars().collect::<Vec<_>>(),
                &"cot".chars().collect::<Vec<_>>()
            )
        );

        assert_eq!(
            1,
            distance(
                &"cat".chars().collect::<Vec<_>>(),
                &"ct".chars().collect::<Vec<_>>()
            )
        );

        assert_eq!(
            1,
            distance(
                &"cat".chars().collect::<Vec<_>>(),
                &"cta".chars().collect::<Vec<_>>()
            )
        );

        assert_eq!(
            2,
            distance(
                &"cat".chars().collect::<Vec<_>>(),
                &"tac".chars().collect::<Vec<_>>()
            )
        );

        assert_eq!(
            3,
            distance(
                &"cat".chars().collect::<Vec<_>>(),
                &"".chars().collect::<Vec<_>>()
            )
        );

        assert_eq!(
            3,
            distance(
                &"".chars().collect::<Vec<_>>(),
                &"cat".chars().collect::<Vec<_>>()
            )
        );

        assert_eq!(
            0,
            distance(
                &"".chars().collect::<Vec<_>>(),
                &"".chars().collect::<Vec<_>>()
            )
        );
    }
}
