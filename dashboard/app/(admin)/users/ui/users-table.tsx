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
  is_approved: boolean;
  created_at: string;
  last_seen_at: string | null;
  guild?: { derived_level: number; derived_rank: string } | null;
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

  async function approveUser(user: UserItem) {
    setWorkingUserId(user.id);
    await fetch(`/api/admin/users/${user.id}/approve`, { method: "POST" });
    setWorkingUserId(null);
    router.refresh();
  }

  async function rejectUser(user: UserItem) {
    if (!confirm(`Reject and permanently delete account for "${user.username}"?`)) return;
    setWorkingUserId(user.id);
    await fetch(`/api/admin/users/${user.id}/reject`, { method: "POST" });
    setWorkingUserId(null);
    router.refresh();
  }

  function statusBadge(user: UserItem) {
    if (!user.is_approved) {
      return <Badge variant="outline" className="text-xs border-amber-400 text-amber-600">pending</Badge>;
    }
    return (
      <Badge variant={user.is_active ? "default" : "secondary"} className="text-xs">
        {user.is_active ? "active" : "suspended"}
      </Badge>
    );
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
            <TableHead>Level</TableHead>
            <TableHead>Rank</TableHead>
            <TableHead>Joined</TableHead>
            <TableHead className="text-right">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {users.length === 0 ? (
            <TableRow>
              <TableCell colSpan={8} className="py-10 text-center text-sm text-muted-foreground">
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
              <TableCell>{statusBadge(user)}</TableCell>
              <TableCell className="text-sm text-muted-foreground">
                {user.last_seen_at
                  ? new Date(user.last_seen_at).toLocaleString()
                  : "never"}
              </TableCell>
              <TableCell className="text-sm text-muted-foreground">
                {user.guild?.derived_level ?? 0}
              </TableCell>
              <TableCell className="text-sm text-muted-foreground">
                {user.guild?.derived_rank || "—"}
              </TableCell>
              <TableCell className="text-sm text-muted-foreground">
                {new Date(user.created_at).toLocaleDateString()}
              </TableCell>
              <TableCell className="text-right">
                {!user.is_approved ? (
                  <div className="flex justify-end gap-2">
                    <Button
                      disabled={workingUserId === user.id}
                      onClick={() => approveUser(user)}
                      size="sm"
                      type="button"
                      variant="default"
                    >
                      {workingUserId === user.id ? "Updating…" : "Approve"}
                    </Button>
                    <Button
                      disabled={workingUserId === user.id}
                      onClick={() => rejectUser(user)}
                      size="sm"
                      type="button"
                      variant="destructive"
                    >
                      Reject
                    </Button>
                  </div>
                ) : (
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
                )}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
