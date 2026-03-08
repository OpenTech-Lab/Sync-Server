import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

type TrustReviewUser = {
  id: string;
  username: string;
  email: string;
  role: string;
  is_active: boolean;
  is_approved: boolean;
  created_at: string;
  last_seen_at: string | null;
  guild: {
    active_days: number;
    derived_level: number;
    derived_rank: string;
    automation_review_state: string;
    suspicious_activity_streak: number;
    last_human_activity_at: string | null;
    last_active_day: string | null;
  } | null;
};

function reviewStateBadge(state: string | undefined) {
  if (state === "frozen") {
    return <Badge variant="destructive">frozen</Badge>;
  }
  if (state === "challenged") {
    return (
      <Badge className="border-amber-400 text-amber-700" variant="outline">
        challenged
      </Badge>
    );
  }
  return <Badge variant="secondary">clear</Badge>;
}

function formatDateTime(value: string | null) {
  if (!value) {
    return "never";
  }
  return new Date(value).toLocaleString();
}

export function TrustReviewUsersTable({ users }: { users: TrustReviewUser[] }) {
  return (
    <div className="overflow-hidden rounded-lg border">
      <Table>
        <TableHeader className="bg-muted/30">
          <TableRow>
            <TableHead>User</TableHead>
            <TableHead>Review state</TableHead>
            <TableHead>Guild</TableHead>
            <TableHead>Suspicion</TableHead>
            <TableHead>Recent activity</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {users.length === 0 ? (
            <TableRow>
              <TableCell className="py-10 text-center text-sm text-muted-foreground" colSpan={5}>
                No flagged users in the selected guild state.
              </TableCell>
            </TableRow>
          ) : null}
          {users.map((user) => (
            <TableRow key={user.id}>
              <TableCell>
                <p className="font-medium">{user.username}</p>
                <p className="text-xs text-muted-foreground">{user.email}</p>
              </TableCell>
              <TableCell>
                <div className="space-y-1">
                  {reviewStateBadge(user.guild?.automation_review_state)}
                  <p className="text-xs text-muted-foreground">{user.role}</p>
                </div>
              </TableCell>
              <TableCell>
                <p className="font-medium">
                  L{user.guild?.derived_level ?? 0} · {user.guild?.derived_rank ?? "-"}
                </p>
                <p className="text-xs text-muted-foreground">
                  {user.guild?.active_days ?? 0} active days
                </p>
              </TableCell>
              <TableCell>
                <p className="font-medium">
                  {user.guild?.suspicious_activity_streak ?? 0} streak
                </p>
                <p className="text-xs text-muted-foreground">
                  Last active day: {user.guild?.last_active_day ?? "n/a"}
                </p>
              </TableCell>
              <TableCell>
                <p className="text-sm text-muted-foreground">
                  Human: {formatDateTime(user.guild?.last_human_activity_at ?? null)}
                </p>
                <p className="text-xs text-muted-foreground">
                  Seen: {formatDateTime(user.last_seen_at)}
                </p>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
