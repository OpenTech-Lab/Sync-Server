import { Badge } from "@/components/ui/badge";
import { requireAdminSession } from "@/lib/session";

import { AdminNav } from "./ui/admin-nav";
import { LogoutButton } from "./ui/logout-button";

const navItems = [
  { href: "/dashboard", label: "Overview" },
  { href: "/trust", label: "Trust Review" },
  { href: "/users", label: "Users" },
  { href: "/stickers", label: "Stickers" },
  { href: "/config", label: "Config" },
  { href: "/planet-news", label: "Planet News" },
  { href: "/audit", label: "Audit Logs" },
];

export default async function AdminLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  const { user } = await requireAdminSession();

  return (
    <div className="flex min-h-screen flex-col bg-background">
      <header className="sticky top-0 z-50 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/80">
        <div className="flex h-14 items-center gap-4 px-6">
          <span className="text-sm font-semibold tracking-tight">Sync Admin</span>
          <div className="flex-1" />
          <div className="flex items-center gap-3">
            <span className="hidden text-sm text-muted-foreground sm:inline">{user.username}</span>
            <Badge className="text-xs" variant="outline">{user.role}</Badge>
            <LogoutButton />
          </div>
        </div>
      </header>

      <div className="flex flex-1">
        <aside className="hidden w-52 shrink-0 border-r md:block">
          <div className="sticky top-14 overflow-y-auto p-4">
            <p className="mb-2 px-2 text-[11px] font-semibold tracking-widest text-muted-foreground/70 uppercase">
              Menu
            </p>
            <AdminNav items={navItems} />
          </div>
        </aside>

        <main className="min-w-0 flex-1 p-6 lg:px-8">
          <div className="mx-auto max-w-5xl space-y-6">{children}</div>
        </main>
      </div>
    </div>
  );
}
