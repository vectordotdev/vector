// Posts or updates the PR comment and commit status for a manually triggered E2E test.
// See action.yml for inputs.
const fs = require("fs");
const path = require("path");

const MAX_FAILURE_LINES = 5;
const MAX_LINE_LENGTH = 300;

const HEADLINES = {
  started: ["⏳", "running"],
  success: ["✅", "passed"],
  failure: ["❌", "failed"],
  cancelled: ["⚪", "was cancelled"]
};

const STATUSES = {
  started: ["pending", "Running"],
  success: ["success", "Passed"],
  failure: ["failure", "Failed"],
  cancelled: ["error", "Cancelled"]
};

// Keeps failure text from breaking the Markdown table.
function cell(text) {
  const line = text.length > MAX_LINE_LENGTH ? `${text.slice(0, MAX_LINE_LENGTH)}…` : text;
  return line.replace(/\|/g, "\\|");
}

// Lists expected results first, in the given order, followed by any unexpected ones.
function readResults(resultsDir, expected) {
  const results = new Map(expected.map((entry) => [entry, { name: entry, missing: true }]));
  if (resultsDir && fs.existsSync(resultsDir)) {
    for (const entry of fs.readdirSync(resultsDir).sort()) {
      const file = path.join(resultsDir, entry, "result.json");
      if (!fs.existsSync(file)) {
        continue;
      }
      try {
        results.set(entry, JSON.parse(fs.readFileSync(file, "utf8")));
      } catch (error) {
        results.set(entry, { name: entry, passed: false, failures: [`Unreadable result.json: ${error.message}`] });
      }
    }
  }
  return [...results.values()];
}

function resultRow(result) {
  if (result.missing) {
    return `| ${cell(result.name)} | ⚪ Not run |`;
  }
  if (result.passed) {
    return `| ${cell(result.name)} | ✅ Passed |`;
  }
  const failures = (result.failures || []).filter((line) => line.trim() !== "");
  const shown = failures.slice(0, MAX_FAILURE_LINES).map(cell);
  if (failures.length > shown.length) {
    shown.push(`…and ${failures.length - shown.length} more`);
  }
  return `| ${cell(result.name)} | ❌ ${shown.join("<br>") || "Failed"} |`;
}

// Collapses this test's earlier reports as outdated so only the newest one stays expanded.
async function hideOlderReports({ github, owner, repo, issueNumber, marker, current }) {
  const comments = await github.paginate(github.rest.issues.listComments, {
    owner,
    repo,
    issue_number: issueNumber,
    per_page: 100
  });
  const older = comments.filter(
    (comment) =>
      comment.id !== current.id && comment.user.login === current.user.login && comment.body.startsWith(marker)
  );
  if (older.length === 0) {
    return;
  }
  const { nodes } = await github.graphql(
    "query($ids: [ID!]!) { nodes(ids: $ids) { ... on IssueComment { id isMinimized } } }",
    { ids: older.map((comment) => comment.node_id) }
  );
  for (const node of nodes.filter((node) => node && !node.isMinimized)) {
    await github.graphql(
      "mutation($id: ID!) { minimizeComment(input: { subjectId: $id, classifier: OUTDATED }) { clientMutationId } }",
      { id: node.id }
    );
  }
}

module.exports = async ({ github, context, core }) => {
  if (context.eventName !== "pull_request_review") {
    core.info(`Skipping PR report for ${context.eventName} event`);
    return;
  }

  const env = process.env;
  const testName = env.TEST_NAME;
  const outcome = env.STATE === "started" ? "started" : env.JOB_STATUS || "failure";
  const [icon, verb] = HEADLINES[outcome] || HEADLINES.failure;
  const [statusState, statusDescription] = STATUSES[outcome] || STATUSES.failure;
  const { owner, repo } = context.repo;
  const issueNumber = context.payload.pull_request.number;
  const sha = context.payload.review.commit_id;
  const runUrl = `${context.serverUrl}/${owner}/${repo}/actions/runs/${context.runId}`;

  const links = [`Commit \`${sha.slice(0, 10)}\``, `[Run](${runUrl})`];
  if (env.ARTIFACT_URL) {
    links.push(`[Evidence](${env.ARTIFACT_URL})`);
  }
  const marker = `<!-- e2e-pr-report:${testName} -->`;
  const lines = [marker, `### ${icon} \`${testName}\` ${verb}`, "", links.join(" · ")];
  if (env.STATE !== "started") {
    const expected = env.EXPECTED_RESULTS.split(/\s+/).filter(Boolean);
    const results = readResults(env.RESULTS_DIR, expected);
    lines.push("");
    if (results.length === 0) {
      lines.push("No results were produced. See the run for details.");
    } else {
      lines.push("| Result | Status |", "| --- | --- |", ...results.map(resultRow));
    }
  }
  const body = lines.join("\n");

  let commentId = parseInt(env.COMMENT_ID, 10);
  if (Number.isNaN(commentId)) {
    const { data: comment } = await github.rest.issues.createComment({
      owner,
      repo,
      issue_number: issueNumber,
      body
    });
    commentId = comment.id;
    try {
      await hideOlderReports({ github, owner, repo, issueNumber, marker, current: comment });
    } catch (error) {
      core.warning(`Could not hide older ${testName} reports: ${error.message}`);
    }
  } else {
    await github.rest.issues.updateComment({ owner, repo, comment_id: commentId, body });
  }
  core.setOutput("comment-id", String(commentId));

  await github.rest.repos.createCommitStatus({
    owner,
    repo,
    sha,
    state: statusState,
    context: `e2e/${testName}`,
    description: statusDescription,
    target_url: runUrl
  });
};
