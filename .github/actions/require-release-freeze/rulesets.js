function readRulesetIds(env) {
  const parseId = (value, variable) => {
    const text = value?.trim();
    const id = Number(text);
    if (!/^[1-9]\d*$/.test(text ?? "") || !Number.isSafeInteger(id)) {
      throw new Error(`${variable} must contain positive integer ruleset IDs.`);
    }
    return id;
  };
  const freezeId = parseId(env.RELEASE_FREEZE_RULESET_ID, "RELEASE_FREEZE_RULESET_ID");
  const bypassIds = (env.RELEASE_FREEZE_BOT_BYPASS ?? "")
    .split(",")
    .map((value) => parseId(value, "RELEASE_FREEZE_BOT_BYPASS"));
  if (bypassIds.includes(freezeId)) {
    throw new Error(
      "RELEASE_FREEZE_BOT_BYPASS must not include RELEASE_FREEZE_RULESET_ID; its bypasses stay unchanged."
    );
  }
  if (new Set(bypassIds).size !== bypassIds.length) {
    throw new Error("RELEASE_FREEZE_BOT_BYPASS must not contain duplicate ruleset IDs.");
  }
  return { freezeId, bypassIds };
}

// Only exact master/default-branch scopes are safe to modify: a bypass on a
// wildcard or multi-branch ruleset would also grant access outside the release.
function targetsOnlyMaster(ruleset, defaultBranch) {
  const refs = ruleset.conditions?.ref_name;
  return (
    ruleset.target === "branch" &&
    refs?.include?.length > 0 &&
    refs.include.every(
      (ref) => ref === "refs/heads/master" || (ref === "~DEFAULT_BRANCH" && defaultBranch === "master")
    ) &&
    refs.exclude?.length === 0
  );
}

function isBotActor(actor, appId) {
  return actor.actor_type === "Integration" && actor.actor_id === appId;
}

function hasBotBypass(ruleset, appId) {
  return ruleset.bypass_actors?.some(
    (actor) => isBotActor(actor, appId) && ["always", "exempt"].includes(actor.bypass_mode)
  );
}

// Identity comes from configuration, not the ruleset's name or bypass actors.
function restrictsMasterUpdates(ruleset, defaultBranch) {
  return (
    ruleset.source_type === "Repository" &&
    targetsOnlyMaster(ruleset, defaultBranch) &&
    ruleset.rules?.some((rule) => rule.type === "update" && rule.parameters?.update_allows_fetch_and_merge !== true)
  );
}

// Identity comes from configuration, not the ruleset's name or bypass actors.
function isReleasePolicy(ruleset, defaultBranch) {
  const allowed = [
    "creation",
    "required_linear_history",
    "update",
    "pull_request",
    "required_status_checks",
    "merge_queue"
  ];
  return (
    ruleset.source_type === "Repository" &&
    targetsOnlyMaster(ruleset, defaultBranch) &&
    ruleset.rules?.length > 0 &&
    ruleset.rules.every((rule) => allowed.includes(rule.type))
  );
}

// Granting a bypass requires the policy to be in force; revoking one does not.
function canManageReleaseBypass(ruleset, defaultBranch) {
  return ruleset.enforcement === "active" && isReleasePolicy(ruleset, defaultBranch);
}

module.exports = {
  readRulesetIds,
  isBotActor,
  hasBotBypass,
  restrictsMasterUpdates,
  isReleasePolicy,
  canManageReleaseBypass
};
