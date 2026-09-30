import { products } from '@/data/products';
import { ProductCard } from '@/components/ProductCard';

export function ShopPage() {
  return (
    <div className="pt-24 pb-16 px-6 max-w-6xl mx-auto">
      <div className="mb-12">
        <h1 className="text-4xl font-bold mb-4">Shop All Products</h1>
        <p className="text-muted-foreground text-lg">
          Discover our curated collection of minimalist home goods.
        </p>
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6">
        {products.map((product) => (
          <ProductCard key={product.id} product={product} />
        ))}
      </div>
    </div>
  );
}
