import { useState } from 'react';
import { products } from '@/data/products';
import { ProductCard } from '@/components/ProductCard';
import { CartDrawer } from '@/components/CartDrawer';
import { ArrowRight } from 'lucide-react';
import { Link } from 'react-router-dom';

export function HomePage() {
  const [cartOpen, setCartOpen] = useState(false);
  const featuredProducts = products.slice(0, 3);

  return (
    <>
      <CartDrawer isOpen={cartOpen} onClose={() => setCartOpen(false)} />
      
      <div className="animate-in fade-in slide-in-from-bottom-4 duration-1000">
        {/* Hero Section */}
        <section className="min-h-[calc(100vh-4rem)] flex flex-col items-center justify-center p-8 text-center">
          <div className="max-w-3xl">
            <span className="inline-block px-4 py-1.5 bg-primary/10 text-primary rounded-full text-sm font-medium mb-6">
              New Collection Available
            </span>
            <h1 className="text-5xl md:text-7xl font-heading font-bold mb-6 tracking-tight leading-tight">
              Thoughtfully<br />
              <span className="text-primary">Curated</span> Goods
            </h1>
            <p className="text-xl text-muted-foreground mb-10 max-w-xl mx-auto leading-relaxed">
              Minimalist home goods designed for modern living. 
              Quality materials, timeless aesthetics.
            </p>
            <div className="flex flex-col sm:flex-row gap-4 justify-center">
              <Link
                to="/shop"
                className="inline-flex items-center justify-center gap-2 bg-primary text-primary-foreground px-8 py-4 rounded-2xl font-semibold text-lg transition-all hover:scale-105 active:scale-95 shadow-lg shadow-primary/20"
              >
                Shop Collection <ArrowRight className="w-5 h-5" />
              </Link>
              <button
                onClick={() => setCartOpen(true)}
                className="px-8 py-4 rounded-2xl font-semibold text-lg border-2 border-border hover:border-primary/50 transition-colors"
              >
                View Cart
              </button>
            </div>
          </div>
        </section>

        {/* Divider */}
        <div className="h-px bg-gradient-to-r from-transparent via-border to-transparent w-full" />

        {/* Featured Products */}
        <section className="p-16 max-w-6xl mx-auto">
          <div className="flex items-end justify-between mb-10">
            <div>
              <h2 className="text-3xl font-bold mb-2">Featured Products</h2>
              <p className="text-muted-foreground">Our most popular items</p>
            </div>
            <Link
              to="/shop"
              className="text-primary font-medium hover:underline flex items-center gap-2"
            >
              View All <ArrowRight className="w-4 h-4" />
            </Link>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
            {featuredProducts.map((product) => (
              <ProductCard key={product.id} product={product} />
            ))}
          </div>
        </section>

        {/* Categories Banner */}
        <section className="p-16 bg-muted/30">
          <div className="max-w-6xl mx-auto text-center">
            <h2 className="text-3xl font-bold mb-4">Shop by Category</h2>
            <div className="flex flex-wrap justify-center gap-4 mt-8">
              {['Lighting', 'Home Decor', 'Textiles', 'Furniture', 'Accessories'].map((category) => (
                <Link
                  key={category}
                  to={`/shop?category=${category.toLowerCase()}`}
                  className="px-6 py-3 bg-background rounded-full border border-border hover:border-primary/50 hover:text-primary transition-colors font-medium"
                >
                  {category}
                </Link>
              ))}
            </div>
          </div>
        </section>
      </div>
    </>
  );
}
