from datetime import datetime, timedelta, timezone
from email.utils import format_datetime
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from repository import dated_release, next_release_date, release_date
from rollback import decode_plan, plan_bytes
from storage import PublicationError


class RollbackDateTests(unittest.TestCase):
    def setUp(self):
        self.now = datetime(2026, 10, 20, 12, tzinfo=timezone.utc)

    def root(self, date):
        return ("Suite:\nCodename: stable\nDate: " + format_datetime(date)
                + "\nSHA256:\n abc 3 vector-0/binary-amd64/Packages\n").encode()

    def test_new_date_supersedes_cached_and_interrupted_roots_without_changing_identity(self):
        old = self.root(self.now - timedelta(days=30))
        cached = self.root(self.now)
        pending = self.root(self.now + timedelta(seconds=1))
        refreshed = dated_release(old, next_release_date([old, cached, pending], self.now))
        self.assertGreater(release_date(refreshed), release_date(pending))
        self.assertEqual(dated_release(refreshed, "DATE"), dated_release(old, "DATE"))

    def test_normal_release_immediately_after_rollback_also_advances(self):
        rollback = self.root(self.now + timedelta(seconds=1))
        later = self.root(self.now + timedelta(seconds=2))
        self.assertEqual(next_release_date([rollback], self.now), format_datetime(release_date(later)))

    def test_future_clock_drift_is_rejected_instead_of_disabling_apt_checks(self):
        with self.assertRaises(PublicationError):
            next_release_date([self.root(self.now + timedelta(minutes=1))], self.now)

    def test_ambiguous_date_or_expiry_policy_is_rejected(self):
        root = self.root(self.now)
        for extra in (b"Date: Tue, 20 Oct 2026 12:00:01 +0000\n", b"Valid-Until: Wed, 21 Oct 2026 12:00:00 +0000\n"):
            with self.assertRaises(PublicationError):
                dated_release(root + extra, next_release_date([], self.now))

    def test_pending_publication_can_only_be_abandoned_to_last_completed_selection(self):
        active, pending, other = "a" * 64, "b" * 64, "c" * 64
        plan, _ = decode_plan(plan_bytes(active, active, pending))
        self.assertEqual(plan["pending"], pending)
        with self.assertRaises(PublicationError):
            plan_bytes(other, active, pending)
        with self.assertRaises(PublicationError):
            plan_bytes("../escape", active, None)


if __name__ == "__main__":
    unittest.main()
