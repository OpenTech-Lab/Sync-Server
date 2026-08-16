import Image from "next/image";
import Link from "next/link";

import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarInset,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { requireAdminSession } from "@/lib/session";

import { AdminNav, type AdminNavItem } from "./ui/admin-nav";
import { NavUser } from "./ui/nav-user";

const navItems: AdminNavItem[] = [
  { href: "/dashboard", label: "Overview", icon: "overview", section: "workspace" },
  { href: "/guild", label: "Guild review", icon: "guild", section: "operations" },
  { href: "/users", label: "Users", icon: "users", section: "operations" },
  { href: "/moderation", label: "Moderation", icon: "moderation", section: "operations" },
  { href: "/stickers", label: "Stickers", icon: "stickers", section: "operations" },
  { href: "/config", label: "Configuration", icon: "config", section: "system" },
  { href: "/planet-news", label: "Planet news", icon: "news", section: "system" },
  { href: "/agent-access", label: "Agent access", icon: "agents", section: "system" },
  { href: "/audit", label: "Audit logs", icon: "audit", section: "system" },
];

export default async function AdminLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  const { user } = await requireAdminSession();

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader className="border-b border-sidebar-border p-3">
          <Link
            className="group flex items-center gap-3 rounded-lg px-1 py-1.5 outline-none transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground group-data-[collapsible=icon]:justify-center"
            href="/dashboard"
          >
            <span className="flex size-9 shrink-0 items-center justify-center overflow-hidden rounded-xl bg-muted shadow-sm ring-1 ring-border/70">
              <Image
                alt="Sync"
                className="size-full object-cover transition-transform duration-300 group-hover:scale-105"
                height={36}
                priority
                src="/admin/logo.png"
                width={36}
              />
            </span>
            <span className="min-w-0 group-data-[collapsible=icon]:hidden">
              <span className="block truncate text-sm font-semibold tracking-tight">
                Sync <span className="font-normal text-muted-foreground">/ Admin</span>
              </span>
              <span className="mt-0.5 block truncate text-[10px] text-muted-foreground">
                Instance control room
              </span>
            </span>
          </Link>
        </SidebarHeader>

        <SidebarContent className="gap-0">
          <AdminNav items={navItems} />
        </SidebarContent>

        <SidebarFooter className="border-t border-sidebar-border p-3">
          <NavUser role={user.role} username={user.username} />
        </SidebarFooter>
        <SidebarRail />
      </Sidebar>

      <SidebarInset className="bg-muted/30">
        <main className="min-h-svh px-3 py-4 sm:px-4 lg:px-6 lg:py-6">
          <div className="mb-3 flex items-center">
            <SidebarTrigger className="shrink-0" />
          </div>
          <div className="mx-auto max-w-6xl">{children}</div>
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
