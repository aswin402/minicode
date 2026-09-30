import { Link } from 'react-router-dom';
import { ThemeToggleButton } from '@/components/ThemeToggleButton';
import { ShoppingBag, Home, Info, Mail } from 'lucide-react';
import { useCartStore } from '@/store/useCartStore';

export function Navbar() {
  const itemCount = useCartStore((state) => state.count());

  return (
    <nav className="fixed top-0 left-0 right-0 h-16 border-b border-border/40 bg-background/80 backdrop-blur-md z-50 flex items-center justify-between px-6">
      <div className="flex items-center gap-8">
        <Link to="/" className="text-xl font-heading font-bold tracking-tight text-primary transition-opacity hover:opacity-80">
          MINISTORE
        </Link>
        <div className="hidden md:flex items-center gap-6">
          <Link to="/" className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors flex items-center gap-2">
            <Home className="w-4 h-4" /> Home
          </Link>
          <Link to="/shop" className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors flex items-center gap-2">
            <ShoppingBag className="w-4 h-4" /> Shop
          </Link>
          <Link to="/about" className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors flex items-center gap-2">
            <Info className="w-4 h-4" /> About
          </Link>
        </div>
      </div>
      <div className="flex items-center gap-4">
        <button className="relative p-2 hover:bg-muted rounded-lg transition-colors">
          <ShoppingBag className="w-5 h-5 text-foreground" />
          {itemCount > 0 && (
            <span className="absolute -top-1 -right-1 bg-primary text-primary-foreground text-xs font-bold w-5 h-5 rounded-full flex items-center justify-center">
              {itemCount}
            </span>
          )}
        </button>
        <ThemeToggleButton />
      </div>
    </nav>
  );
}
