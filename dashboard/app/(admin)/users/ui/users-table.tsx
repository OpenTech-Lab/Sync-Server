"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
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
    <Card className="overflow-hidden py-0">
      <Table>
        <TableHeader className="bg-muted/40">
          <TableRow>
            <TableHead>User</TableHead>
            <TableHead>Role</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Last seen</TableHead>
            <TableHead>Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {users.map((user) => (
            <TableRow key={user.id}>
              <TableCell>
                <p className="font-medium">{user.username}</p>
                <p className="text-muted-foreground">{user.email}</p>
              </TableCell>
              <TableCell>{user.role}</TableCell>
              <TableCell>
                <Badge variant={user.is_active ? "default" : "secondary"}>
                  {user.is_active ? "active" : "suspended"}
                </Badge>
              </TableCell>
              <TableCell className="text-muted-foreground">
                {user.last_seen_at
                  ? new Date(user.last_seen_at).toLocaleString()
                  : "never"}
              </TableCell>
              <TableCell>
                <Button
                  disabled={workingUserId === user.id}
                  onClick={() => toggleStatus(user)}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {workingUserId === user.id
                    ? "Updating..."
                    : user.is_active
                      ? "Suspend"
                      : "Activate"}
                </Button>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </Card>
  );
}
