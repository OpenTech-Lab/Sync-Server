import Link from "next/link";

import { requireAdminSession } from "@/lib/session";

import { LogoutButton } from "./ui/logout-button";

const navItems = [
  { href: "/dashboard", label: "Overview" },
  { href: "/users", label: "Users" },
  { href: "/config", label: "Config" },
  { href: "/audit", label: "Audit Logs" },
];

export default async function AdminLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  const { user } = await requireAdminSession();

  return (
    <div className="min-h-screen bg-muted/20">
      <header className="border-b bg-background">
        <div className="mx-auto flex w-full max-w-6xl items-center justify-between px-6 py-4">
          <div>
            <p className="text-sm text-muted-foreground">Sync Admin</p>
            <p className="font-semibold">{user.username}</p>
          </div>
          <LogoutButton />
        </div>
      </header>

      <div className="mx-auto grid w-full max-w-6xl grid-cols-1 gap-6 px-6 py-6 md:grid-cols-[220px_1fr]">
        <aside className="rounded-lg border bg-background p-3">
          <nav className="flex flex-col gap-1">
            {navItems.map((item) => (
              <Link
                className="rounded-md px-3 py-2 text-sm hover:bg-accent"
                href={item.href}
                key={item.href}
              >
                {item.label}
              </Link>
            ))}
          </nav>
        </aside>
        <main>{children}</main>
      </div>
    </div>
  );
}
