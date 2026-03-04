"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

type UserItem = {
  id: string;
  username: string;
  email: string;
  role: string;
  is_active: boolean;
  created_at: string;
  last_seen_at: string | null;
};

export function UsersTable({ users }: { users: UserItem[] }) {
  const router = useRouter();
  const [workingUserId, setWorkingUserId] = useState<string | null>(null);

  async function toggleStatus(user: UserItem) {
    setWorkingUserId(user.id);
    const action = user.is_active ? "suspend" : "activate";
    await fetch(`/api/admin/users/${user.id}/${action}`, { method: "POST" });
    setWorkingUserId(null);
    router.refresh();
  }

  return (
    <div className="overflow-hidden rounded-lg border">
      <Table>
        <TableHeader className="bg-muted/30">
          <TableRow>
            <TableHead>User</TableHead>
            <TableHead>Role</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Last seen</TableHead>
            <TableHead className="text-right">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {users.length === 0 ? (
            <TableRow>
              <TableCell colSpan={5} className="py-10 text-center text-sm text-muted-foreground">
                No users found.
              </TableCell>
            </TableRow>
          ) : null}
          {users.map((user) => (
            <TableRow key={user.id}>
              <TableCell>
                <p className="font-medium">{user.username}</p>
                <p className="text-xs text-muted-foreground">{user.email}</p>
              </TableCell>
              <TableCell className="text-sm text-muted-foreground">{user.role}</TableCell>
              <TableCell>
                <Badge variant={user.is_active ? "default" : "secondary"} className="text-xs">
                  {user.is_active ? "active" : "suspended"}
                </Badge>
              </TableCell>
              <TableCell className="text-sm text-muted-foreground">
                {user.last_seen_at
                  ? new Date(user.last_seen_at).toLocaleString()
                  : "never"}
              </TableCell>
              <TableCell className="text-right">
                <Button
                  disabled={workingUserId === user.id}
                  onClick={() => toggleStatus(user)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {workingUserId === user.id
                    ? "Updating…"
                    : user.is_active
                      ? "Suspend"
                      : "Activate"}
                </Button>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
