import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

type TrustScoreEventItem = {
  id: string;
  user_id: string;
  granter_user_id: string | null;
  event_type: string;
  delta: number;
  reference_id: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
};

function compactId(value: string | null) {
  if (!value) {
    return "-";
  }
  if (value.length <= 12) {
    return value;
  }
  return `${value.slice(0, 8)}…${value.slice(-4)}`;
}

export function TrustScoreEventsTable({ events }: { events: TrustScoreEventItem[] }) {
  return (
    <div className="overflow-hidden rounded-lg border">
      <Table>
        <TableHeader className="bg-muted/30">
          <TableRow>
            <TableHead>Event</TableHead>
            <TableHead>Delta</TableHead>
            <TableHead>User</TableHead>
            <TableHead>Reference</TableHead>
            <TableHead>Recorded</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {events.length === 0 ? (
            <TableRow>
              <TableCell className="py-10 text-center text-sm text-muted-foreground" colSpan={5}>
                No score events recorded yet.
              </TableCell>
            </TableRow>
          ) : null}
          {events.map((event) => (
            <TableRow key={event.id}>
              <TableCell>
                <p className="font-medium">{event.event_type}</p>
                <p className="text-xs text-muted-foreground">
                  granter {compactId(event.granter_user_id)}
                </p>
              </TableCell>
              <TableCell>
                <Badge variant={event.delta >= 0 ? "default" : "destructive"}>
                  {event.delta >= 0 ? `+${event.delta}` : event.delta}
                </Badge>
              </TableCell>
              <TableCell className="font-mono text-xs">{compactId(event.user_id)}</TableCell>
              <TableCell className="font-mono text-xs">{compactId(event.reference_id)}</TableCell>
              <TableCell className="text-sm text-muted-foreground">
                {new Date(event.created_at).toLocaleString()}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
