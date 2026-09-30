import type { Product } from '@/store/useCartStore';
import { useCartStore } from '@/store/useCartStore';
import { ShoppingBag } from 'lucide-react';

interface ProductCardProps {
  product: Product;
}

export function ProductCard({ product }: ProductCardProps) {
  const addItem = useCartStore((state) => state.addItem);

  return (
    <div className="group relative bg-card rounded-2xl border border-border overflow-hidden transition-all hover:shadow-lg hover:border-primary/30">
      <div className="aspect-square overflow-hidden bg-muted">
        <img
          src={product.image}
          alt={product.name}
          className="w-full h-full object-cover transition-transform duration-500 group-hover:scale-105"
        />
      </div>
      <div className="p-4">
        <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          {product.category}
        </span>
        <h3 className="text-lg font-semibold text-foreground mt-1 mb-2">
          {product.name}
        </h3>
        <div className="flex items-center justify-between">
          <span className="text-xl font-bold text-foreground">
            ${product.price}
          </span>
          <button
            onClick={() => addItem(product)}
            className="p-2 bg-primary text-primary-foreground rounded-xl hover:scale-105 active:scale-95 transition-transform"
            aria-label={`Add ${product.name} to cart`}
          >
            <ShoppingBag className="w-5 h-5" />
          </button>
        </div>
      </div>
    </div>
  );
}
