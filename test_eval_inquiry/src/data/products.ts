import type { Product } from '@/store/useCartStore';

export const products: Product[] = [
  {
    id: '1',
    name: 'Minimal Desk Lamp',
    price: 89,
    category: 'Lighting',
    image: 'https://images.unsplash.com/photo-1507473885765-e6ed057f782c?w=400&q=80',
    description: 'Clean-lined LED desk lamp with adjustable brightness and color temperature.',
  },
  {
    id: '2',
    name: 'Ceramic Plant Pot',
    price: 34,
    category: 'Home Decor',
    image: 'https://images.unsplash.com/photo-1485955900006-10f4d324d411?w=400&q=80',
    description: 'Hand-crafted ceramic pot with drainage hole, perfect for succulents.',
  },
  {
    id: '3',
    name: 'Linen Throw Blanket',
    price: 120,
    category: 'Textiles',
    image: 'https://images.unsplash.com/photo-1555041469-a586c61ea9bc?w=400&q=80',
    description: 'Stonewashed linen throw in natural oat. Breathable and cozy.',
  },
  {
    id: '4',
    name: 'Oak Side Table',
    price: 245,
    category: 'Furniture',
    image: 'https://images.unsplash.com/photo-1555041469-a586c61ea9bc?w=400&q=80',
    description: 'Solid oak side table with tapered legs. Minimalist Scandinavian design.',
  },
  {
    id: '5',
    name: 'Brass Wall Clock',
    price: 156,
    category: 'Accessories',
    image: 'https://images.unsplash.com/photo-1563861826100-9cb868fdbe1c?w=400&q=80',
    description: 'Silent quartz movement with brushed brass finish. 12-inch diameter.',
  },
  {
    id: '6',
    name: 'Cotton Canvas Tote',
    price: 28,
    category: 'Accessories',
    image: 'https://images.unsplash.com/photo-1544816155-12df9643f363?w=400&q=80',
    description: 'Heavyweight organic cotton tote with reinforced handles.',
  },
];
