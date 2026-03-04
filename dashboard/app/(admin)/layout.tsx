import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { requireAdminSession } from "@/lib/session";

import { AdminNav } from "./ui/admin-nav";
import { LogoutButton } from "./ui/logout-button";

const navItems = [
  { href: "/dashboard", label: "Overview" },
  { href: "/users", label: "Users" },
  { href: "/stickers", label: "Stickers" },
  { href: "/stickers/manage", label: "Sticker Moderation" },
  { href: "/config", label: "Config" },
  { href: "/planet-news", label: "Planet News" },
  { href: "/audit", label: "Audit Logs" },
];

export default async function AdminLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  const { user } = await requireAdminSession();

  return (
    <div className="min-h-screen bg-muted/30">
      <header className="border-b bg-background/95 backdrop-blur">
        <div className="mx-auto flex w-full max-w-7xl items-center justify-between px-6 py-4">
          <div className="space-y-1">
            <p className="text-sm text-muted-foreground">Sync Admin Dashboard</p>
            <div className="flex items-center gap-2">
              <p className="text-lg font-semibold leading-none">{user.username}</p>
              <Badge variant="outline">{user.role}</Badge>
            </div>
          </div>
          <LogoutButton />
        </div>
      </header>

      <div className="mx-auto grid w-full max-w-7xl grid-cols-1 gap-6 px-6 py-6 md:grid-cols-[240px_1fr]">
        <Card className="h-fit py-0 md:sticky md:top-6">
          <CardContent className="p-4">
            <p className="mb-3 text-xs font-medium tracking-wide text-muted-foreground uppercase">
              Navigation
            </p>
            <AdminNav items={navItems} />
            <Separator className="my-4" />
            <p className="text-xs text-muted-foreground">
              Manage users, content, and server operations from one place.
            </p>
          </CardContent>
        </Card>

        <main className="space-y-6">{children}</main>
      </div>
    </div>
  );
}
